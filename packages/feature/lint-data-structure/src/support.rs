//! What the aggregate-data-structure rules share.
//!
//! Every rule here declares [`HeadFilter::Heads`], so the engine hands it one
//! matched form and no context at all. Three things follow, and this module is
//! all three:
//!
//! - **[`locate`]** answers what the matched form cannot answer about itself:
//!   is it code or unevaluated data, and is it one of the file's top-level
//!   forms. `RuleContext` carries no parent and no depth, so a rule keyed on
//!   `defstruct` is otherwise called on the `(defstruct …)` inside `'(…)` too,
//!   which is a list, not a structure definition.
//! - **[`top_level_heads`]** is the correlation scan. Two rules here ask about
//!   a *pair* of forms — a `defstruct` and the one it `:include`s, a `gethash`
//!   and the `make-hash-table` that produced its table — and this reads each
//!   top-level form's head without materializing a single subtree.
//! - **[`DefstructForm`]** and [`keyword_argument`] read the two shapes every
//!   rule here needs, once, rather than four times over.
//!
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
//!
//! # Cost
//!
//! Nothing in this module runs unless the engine's head index already matched
//! one of this package's nine anchor heads, so a file spelling none of them
//! pays for none of it. Within a file that does, the order matters and is
//! load-bearing: **every rule performs its cheap, local, allocation-free
//! domain check before calling anything here**. [`locate`] descends from the
//! root and [`top_level_heads`] walks every top-level form; a rule that called
//! either before deciding it had a candidate would pay a tree descent per
//! matched node rather than per finding. The `clean/forms/*` benchmarks lint
//! files with zero findings, which is exactly that cost.
//!
//! [`locate`]'s top-level step uses [`SyntaxTree::root_child_span`] rather than
//! `select_path(&Path::root_child(i))`: [`Path`] owns a `Vec`, so the obvious
//! spelling heap-allocates once per sibling scanned.

use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, Selection, SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

/// Where a matched form sits in its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormSite {
    /// The index, among the file's top-level forms, of the one the match sits
    /// in — which is the match itself when [`FormSite::top_level`] is set.
    pub top_level_index: usize,
    /// Whether the reader would evaluate this form, or read it as data.
    pub quoted: bool,
    /// Whether the match *is* one of the file's top-level forms.
    pub top_level: bool,
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are *not* the same thing
/// and a single depth counter cannot tell them apart. A comma is an escape back
/// to code only when a backquote opened one: inside `` `(a ,x) `` the `,x` is
/// evaluated, but inside `'(a ,x)` there is no backquote for the comma to
/// escape, and what it reads as is either literal list structure (Emacs Lisp)
/// or a reader error (SBCL: "comma not inside a backquote"). In neither case is
/// it code, so `hard` never clears and only `quasi` counts down.
///
/// The same model — deliberately, down to the field names — as
/// `paredit-feature-lint-condition-system`'s and
/// `paredit-feature-lint-object-system`'s. Consolidating the several quote
/// walks in the tree is its own ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside the node at `span`, given the state outside it and the
    /// node's own reader prefixes, read off the source because a [`Selection`]
    /// does not carry them.
    ///
    /// `#'` is deliberately neutral: `#'foo` is a function designator, not
    /// data, and the form under it is still code. So are `#+`/`#-`, which in
    /// this tree are not prefixes at all — a Common Lisp reader conditional
    /// folds into a *single atom* carrying its own `#+sbcl` text, so there is
    /// no prefix here to misread.
    fn after_prefixes_in(mut self, source: &str, span: ByteSpan) -> Self {
        let mut rest = source
            .get(span.start().get()..span.end().get())
            .unwrap_or_default();
        loop {
            rest = rest.trim_start();
            let width = match rest.as_bytes() {
                [b'#', b'\'', ..] => 2,
                [b',', b'@', ..] => {
                    self.quasi = self.quasi.saturating_sub(1);
                    2
                }
                [b',', ..] => {
                    self.quasi = self.quasi.saturating_sub(1);
                    1
                }
                [b'\'', ..] => {
                    self.hard = true;
                    1
                }
                [b'`', ..] => {
                    self.quasi += 1;
                    1
                }
                _ => return self,
            };
            rest = &rest[width..];
        }
    }
}

/// Whether stepping into operand `index` of the form at `path` steps into a
/// spelled-out `(quote …)`, which is `'…` with letters instead of punctuation.
fn spelled_quote_step(
    tree: &SyntaxTree,
    parent: &SexprPath,
    index: usize,
    state: QuoteState,
) -> QuoteState {
    if index == 0 {
        return state;
    }
    let head = tree
        .select_path(parent)
        .ok()
        .and_then(Selection::head)
        .unwrap_or_default();
    let mut next = state;
    if symbol_is(head, "quote") {
        next.hard = true;
    } else if symbol_is(head, "quasiquote") || symbol_is(head, "backquote") {
        next.quasi += 1;
    }
    next
}

/// The child of `parent` (or of the virtual root) that contains `span`, with
/// the path that reaches it.
///
/// Siblings are in source order, so a child starting past the target ends the
/// search: the remaining ones cannot contain it either.
///
/// The root case reads [`SyntaxTree::root_child_span`], which is a slice index
/// and a field read; the `select_path` spelling would allocate an
/// [`SexprPath`]'s `Vec` per sibling scanned.
fn child_containing(
    tree: &SyntaxTree,
    parent: Option<&SexprPath>,
    span: ByteSpan,
) -> Option<(usize, SexprPath, ByteSpan)> {
    let mut index = 0usize;
    loop {
        let child_span = match parent {
            None => tree.root_child_span(index)?,
            Some(path) => tree.select_path(&path.child(index)).ok()?.span(),
        };
        if child_span.start().get() > span.start().get() {
            return None;
        }
        if span.end().get() <= child_span.end().get() {
            let child =
                parent.map_or_else(|| SexprPath::root_child(index), |path| path.child(index));
            return Some((index, child, child_span));
        }
        index += 1;
    }
}

/// Locates `span` — a span that came from a matched [`ExpressionView`] — by
/// descending from the root, accumulating quote state on the way down.
///
/// The cost is the node's depth times its siblings, with no subtree
/// materialized. `None` when no node has exactly that span, which cannot happen
/// for a span the engine handed a rule, and which every caller treats as "do
/// not report".
///
/// **Call this only once a rule already has a candidate.** It descends from the
/// root; calling it to decide whether to look is the ordering mistake that
/// turns a 30ns rule into a 400µs one.
#[must_use]
pub fn locate(tree: &SyntaxTree, span: ByteSpan) -> Option<FormSite> {
    let source = tree.source();
    let mut parent: Option<SexprPath> = None;
    let mut state = QuoteState::EVALUATED;
    let mut top_level_index = 0usize;
    let mut descended = 0usize;

    loop {
        let (index, child_path, child_span) = child_containing(tree, parent.as_ref(), span)?;
        if descended == 0 {
            top_level_index = index;
        }
        if let Some(parent_path) = &parent {
            state = spelled_quote_step(tree, parent_path, index, state);
        }
        state = state.after_prefixes_in(source, child_span);

        if child_span == span {
            return Some(FormSite {
                top_level_index,
                quoted: state.is_data(),
                top_level: descended == 0,
            });
        }
        parent = Some(child_path);
        descended += 1;
    }
}

/// The node whose span is exactly `span`, materialized.
///
/// The way back from a [`Binding`]'s `init_form` — which the binding table
/// carries as a bare span — to the form itself, so a rule can ask what the
/// initializer *calls* rather than matching its source text.
///
/// Costs one descent, no subtree materialized until the last step. `None` when
/// no node has exactly that span.
///
/// [`Binding`]: paredit_core_semantics::semantics::binding::Binding
#[must_use]
pub fn view_at_span(tree: &SyntaxTree, span: ByteSpan) -> Option<ExpressionView> {
    let mut parent: Option<SexprPath> = None;
    loop {
        let (_, child_path, child_span) = child_containing(tree, parent.as_ref(), span)?;
        if child_span == span {
            return Some(tree.select_path(&child_path).ok()?.view());
        }
        parent = Some(child_path);
    }
}

/// Whether the file's text could possibly contain `(setf name …)` or
/// `(setq name …)` — a pre-filter for the walk that answers it properly.
///
/// Two rules here have to ask "is this global reassigned anywhere in the file"
/// before trusting the `make-hash-table`/`make-array` they can see. Answering
/// it needs a walk over every top-level form, and `top_level_view` materializes
/// a whole subtree, so asking it per candidate re-materializes the file.
///
/// This is a byte scan with no allocation and no tree access: it looks for the
/// literal token `setf` or `setq` followed, past whitespace, by the name. It is
/// a *pre-filter* and only ever skips work that could not have found anything —
/// an assignment written `(setf *x* …)` always has that text, so a `false`
/// here is always correct. It deliberately does **not** try to be exact:
/// `(setf (gethash k *x*) v)` writes a hash-table entry rather than the
/// variable, and matches nothing here, which is the case that makes the filter
/// worth having.
///
/// Case-folded, because the reader upcases and source may spell either.
#[must_use]
pub fn might_assign(source: &str, name: &str) -> bool {
    for start in 0..source.len() {
        let Some(head) = source.get(start..start + 4) else {
            continue;
        };
        if !head.eq_ignore_ascii_case("setf") && !head.eq_ignore_ascii_case("setq") {
            continue;
        }
        let rest = source[start + 4..].trim_start();
        if rest
            .get(..name.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        {
            return true;
        }
    }
    false
}

/// One of the file's top-level forms: its index and its head symbol.
#[derive(Debug, Clone, Copy)]
pub struct TopLevelHead<'a> {
    pub index: usize,
    pub head: &'a str,
}

/// Every top-level form's head, without materializing a subtree.
///
/// The correlation scan two rules here need: `defstruct-include-type-mismatch`
/// wants the file's other `defstruct` forms, and
/// `literal-string-key-in-eql-hash-table` wants its `make-hash-table` calls.
/// Both filter this by head first and materialize only the survivors.
pub fn top_level_heads(tree: &SyntaxTree) -> impl Iterator<Item = TopLevelHead<'_>> {
    (0..tree.root_children().len()).filter_map(move |index| {
        let head = tree
            .select_path(&SexprPath::root_child(index))
            .ok()
            .and_then(Selection::head)?;
        Some(TopLevelHead { index, head })
    })
}

/// The top-level form at `index`, materialized.
#[must_use]
pub fn top_level_view(tree: &SyntaxTree, index: usize) -> Option<ExpressionView> {
    Some(tree.select_path(&SexprPath::root_child(index)).ok()?.view())
}

/// The comparison key for a name: the reader upcases unescaped symbols, and a
/// package prefix does not change which name is meant.
#[must_use]
pub fn key(name: &str) -> String {
    unqualified(name).to_ascii_lowercase()
}

/// Whether `text` is a keyword spelled `name`, e.g. `:test` for `"test"`.
///
/// Case-folded, because the reader upcases `:TEST` and `:test` alike.
#[must_use]
pub fn is_keyword(text: &str, name: &str) -> bool {
    text.strip_prefix(':')
        .is_some_and(|rest| rest.eq_ignore_ascii_case(name))
}

/// The value operand following the keyword `name` in `view`'s operands, scanned
/// from `first` as a keyword plist.
///
/// Returns the *view*, so a caller can ask whether the value is a literal, a
/// symbol, or a call.
#[must_use]
pub fn keyword_argument<'a>(
    view: &'a ExpressionView,
    first: usize,
    name: &str,
) -> Option<&'a ExpressionView> {
    let mut index = first;
    while index + 1 < view.children.len() {
        if atom_text(&view.children[index]).is_some_and(|text| is_keyword(text, name)) {
            return Some(&view.children[index + 1]);
        }
        index += 2;
    }
    None
}

/// Whether the keyword `name` appears at all in `view`'s operands from `first`,
/// read as a keyword plist.
#[must_use]
pub fn has_keyword(view: &ExpressionView, first: usize, name: &str) -> bool {
    let mut index = first;
    while index + 1 < view.children.len() {
        if atom_text(&view.children[index]).is_some_and(|text| is_keyword(text, name)) {
            return true;
        }
        index += 2;
    }
    false
}

/// A `defstruct`'s name-and-options header, read once.
///
/// `(defstruct name …)` and `(defstruct (name option…) …)` are the two spellings
/// CLHS gives, and every rule here needs to tell them apart before it can read
/// anything else.
#[derive(Debug, Clone)]
pub struct DefstructForm<'a> {
    /// The structure name, past any options list.
    pub name: &'a str,
    /// The options list `(name option…)`, or `None` for the bare-name spelling.
    pub options: Option<&'a ExpressionView>,
    /// The slot descriptions, which are every operand after the header.
    pub slots: Vec<SlotDescription<'a>>,
}

/// One slot description in a `defstruct` body.
#[derive(Debug, Clone, Copy)]
pub struct SlotDescription<'a> {
    pub name: &'a str,
    /// Whether the description supplies an initform: `(name initform …)` does,
    /// a bare `name` and a lone `(name)` do not.
    pub has_initform: bool,
}

/// Reads `(defstruct …)`'s header and slot list, or `None` if `view` is not a
/// `defstruct` call at all.
///
/// A docstring first operand is skipped: CLHS allows `(defstruct foo "doc"
/// slot…)`, and reading the docstring as a slot would invent a slot named by
/// the whole string.
#[must_use]
pub fn defstruct_form<'a>(view: &'a ExpressionView) -> Option<DefstructForm<'a>> {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "defstruct")) {
        return None;
    }
    let header = view.children.get(1)?;
    let (name, options) = match atom_text(header) {
        Some(name) => (name, None),
        None if is_paren_list(header) => (atom_text(header.children.first()?)?, Some(header)),
        None => return None,
    };

    let mut rest = &view.children[2.min(view.children.len())..];
    // `(defstruct foo "doc" a b)` — the docstring is not a slot.
    if rest
        .first()
        .and_then(atom_text)
        .is_some_and(|text| text.starts_with('"'))
    {
        rest = &rest[1..];
    }

    let slots = rest
        .iter()
        .filter_map(|slot| match atom_text(slot) {
            Some(name) => Some(SlotDescription {
                name,
                has_initform: false,
            }),
            None if is_paren_list(slot) => Some(SlotDescription {
                name: atom_text(slot.children.first()?)?,
                has_initform: slot.children.len() >= 2,
            }),
            None => None,
        })
        .collect();

    Some(DefstructForm {
        name,
        options,
        slots,
    })
}

impl DefstructForm<'_> {
    /// Every `(:option …)` in the header, in written order.
    ///
    /// The bare-name spelling has none, and the bare atom options CLHS allows
    /// in the options list — `:named`, and `:conc-name` written without a
    /// value — are not lists and so are not yielded here.
    pub fn option_forms(&self) -> impl Iterator<Item = &ExpressionView> {
        self.options
            .into_iter()
            .flat_map(|options| options.children.iter().skip(1))
            .filter(|option| is_paren_list(option))
    }

    /// The single option named `name`, e.g. `(:include base …)`.
    #[must_use]
    pub fn option(&self, name: &str) -> Option<&ExpressionView> {
        self.option_forms().find(|option| {
            option
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|text| is_keyword(text, name))
        })
    }

    /// Whether a slot of this name is declared here.
    #[must_use]
    pub fn declares_slot(&self, name: &str) -> bool {
        let wanted = key(name);
        self.slots.iter().any(|slot| key(slot.name) == wanted)
    }
}

/// The `:type` representation a `defstruct` header names, lowercased, or `None`
/// when it declares no `:type` and so is a real structure class.
#[must_use]
pub fn declared_type(form: &DefstructForm<'_>) -> Option<String> {
    let option = form.option("type")?;
    let value = option.children.get(1)?;
    atom_text(value)
        .map(key)
        .or_else(|| Some("(vector …)".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parse(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn only_defstruct(source: &str) -> ExpressionView {
        let tree = parse(source);
        top_level_view(&tree, 0).expect("a top-level form")
    }

    #[test]
    fn reads_the_bare_name_spelling() {
        let view = only_defstruct("(defstruct point x y)");
        let form = defstruct_form(&view).expect("a defstruct");
        assert_eq!(form.name, "point");
        assert!(form.options.is_none());
        assert_eq!(form.slots.len(), 2);
        assert!(!form.slots[0].has_initform);
    }

    #[test]
    fn reads_the_options_list_spelling() {
        let view = only_defstruct("(defstruct (point (:conc-name p-)) (x 0) y)");
        let form = defstruct_form(&view).expect("a defstruct");
        assert_eq!(form.name, "point");
        assert!(form.option("conc-name").is_some());
        assert!(form.slots[0].has_initform, "(x 0) supplies an initform");
        assert!(!form.slots[1].has_initform, "a bare y does not");
    }

    #[test]
    fn a_lone_parenthesised_slot_supplies_no_initform() {
        let view = only_defstruct("(defstruct point (x) (y 0))");
        let form = defstruct_form(&view).expect("a defstruct");
        assert!(!form.slots[0].has_initform, "(x) has no initform");
        assert!(form.slots[1].has_initform);
    }

    #[test]
    fn a_docstring_is_not_read_as_a_slot() {
        let view = only_defstruct("(defstruct point \"A 2D point.\" x y)");
        let form = defstruct_form(&view).expect("a defstruct");
        assert_eq!(form.slots.len(), 2);
        assert_eq!(form.slots[0].name, "x");
    }

    #[test]
    fn reads_the_declared_representation_type() {
        let view = only_defstruct("(defstruct (r (:type list)) a)");
        let form = defstruct_form(&view).expect("a defstruct");
        assert_eq!(declared_type(&form).as_deref(), Some("list"));
    }

    #[test]
    fn a_structure_with_no_type_option_has_no_declared_type() {
        let view = only_defstruct("(defstruct r a)");
        let form = defstruct_form(&view).expect("a defstruct");
        assert!(declared_type(&form).is_none());
    }

    #[test]
    fn keyword_arguments_are_read_as_a_plist_and_case_folded() {
        let view = only_defstruct("(make-array 3 :INITIAL-ELEMENT 0 :adjustable t)");
        assert!(keyword_argument(&view, 2, "initial-element").is_some());
        assert!(has_keyword(&view, 2, "adjustable"));
        assert!(!has_keyword(&view, 2, "fill-pointer"));
    }

    /// `:initial-element :fill-pointer` stores a keyword; it does not declare
    /// a fill pointer. The plist *stride* is what gets this right.
    ///
    /// Mutation testing found the first spelling of this test — with
    /// `:fill-pointer` as the final operand — killed nothing: the scan's
    /// `index + 1 < len` bound stops before the last element either way, so a
    /// stride of 1 passed it. A trailing value is what forces the stride to
    /// matter.
    #[test]
    fn a_keyword_in_a_value_position_is_not_read_as_a_key() {
        let view = only_defstruct("(make-array 3 :initial-element :fill-pointer 0)");
        assert!(
            !has_keyword(&view, 2, "fill-pointer"),
            "the keyword sits in a value position, so it declares nothing"
        );
        assert!(
            keyword_argument(&view, 2, "fill-pointer").is_none(),
            "and it must not yield a value either"
        );
        // The real key at that position is still found, so the stride is not
        // simply skipping everything.
        assert!(has_keyword(&view, 2, "initial-element"));
    }

    #[test]
    fn locate_marks_a_quoted_form_as_data() {
        let tree = parse("(setf x '(defstruct point a))");
        let root = tree.root_view();
        let inner = root.children[0].children[2].children[0].clone();
        let site = locate(&tree, inner.span).expect("located");
        assert!(site.quoted);
        assert!(!site.top_level);
    }

    #[test]
    fn locate_marks_a_top_level_form_as_code() {
        let tree = parse("(defstruct point a)");
        let site = locate(&tree, tree.root_child_span(0).expect("one form")).expect("located");
        assert!(!site.quoted);
        assert!(site.top_level);
    }

    /// A comma inside `'(…)` escapes nothing — there is no backquote for it to
    /// escape. A single depth counter gets this wrong.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_return_to_code() {
        let tree = parse("(setf x '(a ,(defstruct point b)))");
        let root = tree.root_view();
        let inner = root.children[0].children[2].children[1].clone();
        let site = locate(&tree, inner.span).expect("located");
        assert!(site.quoted, "'(a ,(…)) is data all the way down");
    }

    #[test]
    fn a_comma_inside_a_quasiquote_does_return_to_code() {
        let tree = parse("(defmacro m () `(a ,(defstruct point b)))");
        let root = tree.root_view();
        let inner = root.children[0].children[3].children[1].clone();
        let site = locate(&tree, inner.span).expect("located");
        assert!(!site.quoted, "`(a ,(…)) evaluates the comma'd form");
    }

    #[test]
    fn might_assign_finds_a_direct_assignment_in_either_spelling_and_case() {
        assert!(might_assign("(setf *x* 1)", "*x*"));
        assert!(might_assign("(setq *x* 1)", "*x*"));
        assert!(might_assign("(SETF *X* 1)", "*x*"));
        assert!(might_assign("(setf\n  *x* 1)", "*x*"));
    }

    /// The case the filter exists for: writing *through* the variable is not
    /// writing the variable, and must not force the walk.
    #[test]
    fn might_assign_ignores_a_write_through_the_variable() {
        assert!(!might_assign("(setf (gethash k *x*) v)", "*x*"));
        assert!(!might_assign("(setf (aref *x* 0) v)", "*x*"));
    }

    #[test]
    fn might_assign_is_false_when_nothing_assigns_at_all() {
        assert!(!might_assign("(defparameter *x* (make-hash-table))", "*x*"));
        assert!(!might_assign("(setf *other* 1)", "*x*"));
    }

    /// A name that is a prefix of another must not match it, and the scan must
    /// not panic on a multi-byte boundary.
    #[test]
    fn might_assign_survives_non_ascii_source() {
        assert!(!might_assign(
            "(defun f () \"日本語のドキュメント\")",
            "*x*"
        ));
        assert!(might_assign("\"日本語\" (setf *x* 1)", "*x*"));
    }

    #[test]
    fn top_level_heads_reads_every_form_in_order() {
        let tree = parse("(defstruct a x)\n(defun f ())\n(defstruct b y)");
        let heads: Vec<_> = top_level_heads(&tree)
            .map(|form| form.head.to_owned())
            .collect();
        assert_eq!(heads, vec!["defstruct", "defun", "defstruct"]);
    }
}
