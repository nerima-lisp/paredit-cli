//! What the eight CLOS rules in this package share.
//!
//! Three things, all of them consequences of one constraint: every rule here
//! declares [`HeadFilter::Heads`], so the engine hands it a single matched form
//! and nothing else.
//!
//! - **[`locate`]** answers the two questions the matched form cannot answer
//!   about itself: is it code or unevaluated data, and is it a top-level
//!   definition. `RuleContext` carries no parent and no depth, so a rule keyed
//!   on `defclass` is otherwise called on the `(defclass …)` inside `'(…)` too,
//!   which is a list, not a class.
//! - **[`top_level_forms`]** is the correlation scan. Three of these rules ask
//!   about a *pair* of definitions — a class and its superclass, a generic and
//!   its methods, a method and an earlier one — and read each candidate's head
//!   without materializing a single subtree.
//! - **[`class_form`]/[`method_form`]/[`method_signature`]** are the two CLOS
//!   definition forms, read once here rather than re-derived per rule.
//!
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
//!
//! ## Cost
//!
//! Nothing in this module is reached unless the engine's head index already
//! matched a `defclass`, `defgeneric`, `defmethod` or `slot-value` head, so a
//! file containing none of those pays for none of it. Within a file that does,
//! [`locate`] costs one descent (depth × siblings, no subtree materialized) and
//! [`top_level_forms`] costs one span-and-head read per top-level form.

use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix, Selection,
    SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, symbol_is, unqualified,
};

/// Where a matched form sits in its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormSite {
    /// The index, among the file's top-level forms, of the one the match sits
    /// in — which is the match itself when [`FormSite::top_level`] is set.
    pub top_level_index: usize,
    /// Whether the reader would evaluate this form, or read it as data.
    ///
    /// True inside a `` ` ``/`(quasiquote …)` region that no `,`/`,@` escapes,
    /// and true unconditionally inside a `'`/`(quote …)` region — a comma there
    /// escapes nothing, because there is no backquote for it to escape. See
    /// [`QuoteState`]. A `(defmethod …)` inside quoted data defines no method,
    /// and a rule that flags one is reporting on a list literal.
    pub quoted: bool,
    /// Whether the match *is* one of the file's top-level forms.
    pub top_level: bool,
}

/// Locates `span` — a span that came from a matched [`ExpressionView`] — by
/// descending from the root, accumulating quote depth on the way down.
///
/// `None` when no node has exactly that span, which cannot happen for a span
/// the engine handed a rule and is treated by every caller as "do not report".
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
        // A spelled-out `(quote X)` is `'X` with letters instead of
        // punctuation, and the step into its argument has to count the same.
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

/// Whether the node at `span` sits in a *place* — a write position — of one of
/// the `setf`-family macros.
///
/// Descends the same single chain [`locate`] does and looks at exactly one
/// thing: the operand index the target occupies in its immediate parent. Only
/// the immediate parent, because `(setf (slot-value (slot-value a 'b) 'c) v)`
/// writes the outer place and *reads* the inner one to find the object.
///
/// `None` when `span` names no node, which every caller treats the same way
/// [`locate`] returning `None` is treated.
#[must_use]
pub fn is_write_position(tree: &SyntaxTree, span: ByteSpan) -> Option<bool> {
    let mut parent: Option<SexprPath> = None;
    loop {
        let (index, child_path, child_span) = child_containing(tree, parent.as_ref(), span)?;
        if child_span == span {
            let Some(parent_path) = &parent else {
                // A top-level form has no operator above it to write it.
                return Some(false);
            };
            let head = tree
                .select_path(parent_path)
                .ok()
                .and_then(Selection::head)
                .unwrap_or_default();
            return Some(writes_operand(head, index));
        }
        parent = Some(child_path);
    }
}

/// Whether operand `index` of a `(head …)` form is a place the form writes.
///
/// The list is the `setf`-family macros whose expansion assigns to a place, and
/// for each one the operand positions that *are* places. `incf`, `decf` and
/// `push` read their place as well as writing it, but they read it through the
/// same `(setf …)` expansion, so a place with no writer is no more usable there
/// than in a bare `setf`.
fn writes_operand(head: &str, index: usize) -> bool {
    if index == 0 {
        return false;
    }
    if symbol_in(head, &["setf", "psetf"]) {
        // place, value, place, value…
        return index % 2 == 1;
    }
    if symbol_in(head, &["rotatef", "shiftf"]) {
        // Every operand of `rotatef` is a place, and every operand of `shiftf`
        // but the last one is — the last is a value, and distinguishing it
        // would only ever *add* a report, so it is left in.
        return true;
    }
    if symbol_in(head, &["incf", "decf", "pop", "remf"]) {
        return index == 1;
    }
    if symbol_in(head, &["push", "pushnew"]) {
        return index == 2;
    }
    false
}

/// The child of `parent` (or of the virtual root) that contains `span`, with
/// the path that reaches it.
///
/// Siblings are in source order, so a child starting past the target ends the
/// search: the remaining ones cannot contain it either.
fn child_containing(
    tree: &SyntaxTree,
    parent: Option<&SexprPath>,
    span: ByteSpan,
) -> Option<(usize, SexprPath, ByteSpan)> {
    let mut index = 0usize;
    loop {
        let child = parent.map_or_else(|| SexprPath::root_child(index), |path| path.child(index));
        let selection = tree.select_path(&child).ok()?;
        let child_span = selection.span();
        if child_span.start().get() > span.start().get() {
            return None;
        }
        if child_span.start().get() <= span.start().get()
            && span.end().get() <= child_span.end().get()
        {
            return Some((index, child, child_span));
        }
        index += 1;
    }
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are *not* the same thing
/// and a single depth cannot tell them apart. A comma is an escape back to code
/// only when a backquote opened one: inside `` `(a ,x) `` the `,x` is evaluated,
/// but inside `'(a ,x)` there is no backquote to escape, and what the `,` reads
/// as is either literal list structure (Emacs Lisp, `(a (\, x))`) or a reader
/// error (SBCL: "comma not inside a backquote"). In neither case is it code, so
/// `hard` never clears and only `quasi` counts down.
///
/// This is the same model — deliberately, down to the field names — that
/// `paredit-feature-lint-condition-system`'s `support::QuoteState` uses. The two
/// packages carry their own copies rather than a dependency between sibling
/// features; consolidating the several quote walks in the tree is its own
/// ticket.
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

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }

    const fn quasiquoted(mut self) -> Self {
        self.quasi += 1;
        self
    }

    const fn unquoted(mut self) -> Self {
        self.quasi = self.quasi.saturating_sub(1);
        self
    }

    /// The state inside the node at `span`, given the state outside it and the
    /// node's own reader prefixes — read off the source, because a
    /// [`Selection`] does not carry them.
    ///
    /// `#'` is deliberately neutral: `#'foo` is a function designator, not
    /// data, and the form under it is still code.
    fn after_prefixes_in(mut self, source: &str, span: ByteSpan) -> Self {
        let mut rest = source
            .get(span.start().get()..span.end().get())
            .unwrap_or_default();
        loop {
            rest = rest.trim_start();
            let width = match rest.as_bytes() {
                [b'#', b'\'', ..] => 2,
                [b',', b'@', ..] => {
                    self = self.unquoted();
                    2
                }
                [b',', ..] => {
                    self = self.unquoted();
                    1
                }
                [b'\'', ..] => {
                    self = self.quoted();
                    1
                }
                [b'`', ..] => {
                    self = self.quasiquoted();
                    1
                }
                _ => return self,
            };
            rest = &rest[width..];
        }
    }
}

/// The state inside child `index` of the list at `parent`, which differs from
/// the state outside it only for the argument of a spelled-out `(quote X)`,
/// `(quasiquote X)`, `(unquote X)` or `(unquote-splicing X)`.
fn spelled_quote_step(
    tree: &SyntaxTree,
    parent: &SexprPath,
    index: usize,
    state: QuoteState,
) -> QuoteState {
    if index != 1 {
        return state;
    }
    let Some(head) = tree.select_path(parent).ok().and_then(Selection::head) else {
        return state;
    };
    if symbol_is(head, "quote") {
        state.quoted()
    } else if symbol_in(head, &["quasiquote", "backquote"]) {
        state.quasiquoted()
    } else if symbol_in(head, &["unquote", "unquote-splicing"]) {
        state.unquoted()
    } else {
        state
    }
}

/// One top-level form the reader evaluates.
#[derive(Debug, Clone, Copy)]
pub struct TopLevelForm<'a> {
    /// The form's index among the file's top-level forms.
    pub index: usize,
    /// Its head symbol, exactly as written.
    pub head: &'a str,
    pub span: ByteSpan,
}

/// Every top-level `(head …)` form the reader evaluates, in file order.
///
/// Quoted top-level forms are dropped: `'(defclass point () (x))` is a list
/// literal, and a rule correlating class definitions must not resolve a
/// superclass to one.
///
/// Reads only each form's head and span — no subtree is materialized — so
/// correlating definitions costs a scan of the top level, not of the file.
pub fn top_level_forms(tree: &SyntaxTree) -> impl Iterator<Item = TopLevelForm<'_>> {
    let source = tree.source();
    (0..tree.root_children().len()).filter_map(move |index| {
        let selection = tree.select_path(&SexprPath::root_child(index)).ok()?;
        let span = selection.span();
        if QuoteState::EVALUATED
            .after_prefixes_in(source, span)
            .is_data()
        {
            return None;
        }
        Some(TopLevelForm {
            index,
            head: selection.head()?,
            span,
        })
    })
}

/// Materializes one top-level form. Called only after [`top_level_forms`] has
/// already said its head is the one a rule wants.
#[must_use]
pub fn top_level_view(tree: &SyntaxTree, index: usize) -> Option<ExpressionView> {
    tree.select_path(&SexprPath::root_child(index))
        .ok()
        .map(Selection::view)
}

/// One slot of a `defclass` slot list.
#[derive(Debug, Clone)]
pub struct SlotSpec<'a> {
    pub name: &'a str,
    pub span: ByteSpan,
    /// The slot's `(keyword, value)` options, in written order. A trailing
    /// keyword with no value is carried with `None` rather than dropped.
    pub options: Vec<(&'a str, Option<&'a ExpressionView>)>,
}

impl<'a> SlotSpec<'a> {
    /// The value of the first `option` on this slot, if it has one.
    #[must_use]
    pub fn option(&self, option: &str) -> Option<&'a ExpressionView> {
        self.options
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(option))
            .and_then(|(_, value)| *value)
    }

    #[must_use]
    pub fn has_option(&self, option: &str) -> bool {
        self.options
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case(option))
    }

    /// Every `:reader`/`:accessor` name this slot declares — the ways to read
    /// it that are not `slot-value`.
    #[must_use]
    pub fn reader_names(&self) -> Vec<&'a str> {
        self.options
            .iter()
            .filter(|(key, _)| {
                key.eq_ignore_ascii_case(":reader") || key.eq_ignore_ascii_case(":accessor")
            })
            .filter_map(|(_, value)| value.and_then(|view| atom_text(view)))
            .collect()
    }

    /// The slot's allocation, defaulting to `:instance` exactly as CLOS does,
    /// so two slots that differ only by an omitted `:allocation :instance`
    /// compare equal.
    #[must_use]
    pub fn allocation(&self) -> String {
        self.option(":allocation")
            .and_then(|view| atom_text(view))
            .unwrap_or(":instance")
            .to_ascii_lowercase()
    }
}

/// A `(defclass name (super…) (slot…) option…)` form.
#[derive(Debug, Clone)]
pub struct ClassForm<'a> {
    pub name: &'a str,
    pub superclasses: Vec<&'a str>,
    pub slots: Vec<SlotSpec<'a>>,
}

/// Reads a `defclass` form, or `None` when `view` is not one or is missing the
/// positional parts a class needs.
#[must_use]
pub fn class_form(view: &ExpressionView) -> Option<ClassForm<'_>> {
    if !list_head(view).is_some_and(|head| symbol_is(head, "defclass")) {
        return None;
    }
    let name = view.children.get(1).and_then(atom_text)?;
    let superclasses = view.children.get(2).map_or_else(Vec::new, |list| {
        list.children.iter().filter_map(atom_text).collect()
    });
    let slots = view.children.get(3).map_or_else(Vec::new, |list| {
        list.children.iter().filter_map(slot_spec).collect()
    });
    Some(ClassForm {
        name,
        superclasses,
        slots,
    })
}

/// Reads one slot spec: either a bare `x` or a `(x :initarg :x …)` list.
///
/// Options are walked two at a time so a value that happens to look like a
/// keyword — `:allocation :class` — is never read as an option itself. That is
/// the same stride `lint-convention`'s `defclass-slot-option` uses, for the
/// same reason.
fn slot_spec(view: &ExpressionView) -> Option<SlotSpec<'_>> {
    if let Some(name) = atom_text(view) {
        return Some(SlotSpec {
            name,
            span: view.span,
            options: Vec::new(),
        });
    }
    let name = view.children.first().and_then(atom_text)?;
    let mut options = Vec::new();
    let mut index = 1;
    while index < view.children.len() {
        let Some(key) = atom_text(&view.children[index]).filter(|key| key.starts_with(':')) else {
            index += 1;
            continue;
        };
        options.push((key, view.children.get(index + 1)));
        index += 2;
    }
    Some(SlotSpec {
        name,
        span: view.span,
        options,
    })
}

/// A `(defmethod name qualifier… lambda-list body…)` form.
#[derive(Debug, Clone)]
pub struct MethodForm<'a> {
    /// The generic function's name, or `None` for a `(setf name)` method,
    /// whose name is a list rather than a symbol.
    pub name: Option<&'a str>,
    /// The atoms between the name and the lambda list.
    pub qualifiers: Vec<&'a ExpressionView>,
    pub lambda_list: Option<&'a ExpressionView>,
    pub body: &'a [ExpressionView],
}

/// Reads a `defmethod` form, or `None` when `view` is not one.
///
/// The lambda list is the first non-atom after the name, which is exactly how
/// CLOS separates it from the qualifiers: qualifiers are non-list objects, and
/// there is no other way to tell `:around` from a lambda list.
#[must_use]
pub fn method_form(view: &ExpressionView) -> Option<MethodForm<'_>> {
    if !list_head(view).is_some_and(|head| symbol_is(head, "defmethod")) {
        return None;
    }
    let name = view.children.get(1).and_then(atom_text);
    let mut qualifiers = Vec::new();
    let mut index = 2;
    while let Some(child) = view.children.get(index) {
        if child.kind != ExpressionKind::Atom {
            break;
        }
        qualifiers.push(child);
        index += 1;
    }
    let lambda_list = view.children.get(index);
    let body = lambda_list.map_or(&[][..], |_| &view.children[index + 1..]);
    Some(MethodForm {
        name,
        qualifiers,
        lambda_list,
        body,
    })
}

/// A `defmethod`'s dispatch identity: what CLOS compares when deciding whether
/// a second `defmethod` adds a method or *replaces* one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSignature {
    pub name: String,
    pub qualifiers: Vec<String>,
    /// One entry per required parameter, `"t"` for an unspecialized one — the
    /// normalization CLOS itself applies, so `(x)` and `(x t)` compare equal.
    pub specializers: Vec<String>,
}

/// Reads the signature of the top-level form at `index`, if it is a
/// `defmethod`.
///
/// Navigates by path rather than materializing the form, so comparing every
/// method in a file against every earlier one never copies a method body.
#[must_use]
pub fn method_signature(tree: &SyntaxTree, index: usize) -> Option<MethodSignature> {
    let form = SexprPath::root_child(index);
    if !tree
        .select_path(&form)
        .ok()
        .and_then(Selection::head)
        .is_some_and(|head| symbol_is(head, "defmethod"))
    {
        return None;
    }

    let name = normalized(tree.select_path(&form.child(1)).ok()?.text());
    let mut qualifiers = Vec::new();
    let mut child = 2;
    let lambda_list = loop {
        let Ok(selection) = tree.select_path(&form.child(child)) else {
            return None;
        };
        if selection.kind() != ExpressionKind::Atom {
            break selection;
        }
        qualifiers.push(normalized(selection.text()));
        child += 1;
    };

    Some(MethodSignature {
        name,
        qualifiers,
        specializers: specializers_of(&lambda_list.view()),
    })
}

/// The specializer of each required parameter, stopping at the first lambda-list
/// keyword: `&optional` and everything after it is never specialized.
fn specializers_of(lambda_list: &ExpressionView) -> Vec<String> {
    let mut specializers = Vec::new();
    for parameter in &lambda_list.children {
        if atom_text(parameter).is_some_and(|text| text.starts_with('&')) {
            break;
        }
        let specializer = parameter.children.get(1).map_or_else(
            || "t".to_owned(),
            |view| atom_text(view).map_or_else(|| normalized(&render(view)), normalized),
        );
        specializers.push(specializer);
    }
    specializers
}

/// A view's source text, reconstructed from its atoms — enough to compare an
/// `(eql 'x)` specializer against another, which is all it is used for.
fn render(view: &ExpressionView) -> String {
    match atom_text(view) {
        Some(text) => text.to_owned(),
        None => {
            let parts: Vec<String> = view.children.iter().map(render).collect();
            format!("({})", parts.join(" "))
        }
    }
}

/// Case-folded, package-stripped and whitespace-collapsed, so two spellings of
/// one name compare equal: the reader upcases unescaped symbols, and a package
/// prefix does not change which function is named.
///
/// Applied per whitespace token rather than to the whole string, because a
/// `(setf cl-user:width)` name is a *list*, and stripping its package
/// qualification as one string would cut it at the wrong colon.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .map(|token| unqualified(token).to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The normalized name a `defgeneric`, `defmethod` or `defclass` form defines,
/// spelled exactly as [`MethodSignature::name`] spells it — so a generic and
/// the methods that specialize it can be matched by name.
///
/// `None` for a form with no second element at all.
#[must_use]
pub fn definition_name(view: &ExpressionView) -> Option<String> {
    view.children.get(1).map(|name| normalized(&render(name)))
}

/// Whether any form under `view` calls one of `heads`, or names one of them as
/// a bare symbol (`#'call-next-method`, `(apply #'call-next-method args)`).
///
/// The bare-symbol case is deliberate: a rule asking "does this body ever reach
/// `call-next-method`" must answer yes when the body only passes it along.
#[must_use]
pub fn mentions(view: &ExpressionView, heads: &[&str]) -> bool {
    if symbol_text(view).is_some_and(|text| symbol_in(text, heads)) {
        return true;
    }
    view.children.iter().any(|child| mentions(child, heads))
}

/// An atom's own symbol text, without the reader prefix its span includes.
///
/// [`atom_text`] returns the whole span, so `#'call-next-method` reads as
/// `"#'call-next-method"` and matches no symbol at all. `symbol_offset` is
/// where the parser recorded the symbol itself starting.
#[must_use]
pub fn symbol_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view).and_then(|text| text.get(view.symbol_offset..))
}

/// The symbol a quoted symbol designator names: `'x` or `(quote x)`, which are
/// the two spellings of the second argument to `slot-value`.
///
/// `None` for anything else, including an unquoted variable — `(slot-value o
/// name)` reads a slot chosen at runtime, and no static rule can say which.
#[must_use]
pub fn quoted_symbol(view: &ExpressionView) -> Option<&str> {
    if let Some(text) = atom_text(view) {
        return (view.reader_prefixes.as_slice() == [ReaderPrefix::Quote])
            .then(|| text.get(view.symbol_offset..))
            .flatten();
    }
    if !calls_head(view, "quote") || view.children.len() != 2 {
        return None;
    }
    symbol_text(&view.children[1])
}

/// Whether `view`'s own spelling makes it unevaluated data: a `'`/`` ` ``
/// reader prefix, or a spelled-out `(quote …)`.
///
/// The cheap, node-local half of [`locate`], for a walk that already has the
/// whole subtree in hand and only needs to know which branches to stay out of.
/// A quasiquote counts as data even though a `,` inside it escapes back to
/// code: over-skipping loses findings, and that is the safe direction for a
/// rule deciding whether to report.
#[must_use]
pub fn is_quoted_here(view: &ExpressionView) -> bool {
    matches!(
        view.reader_prefixes.first(),
        Some(ReaderPrefix::Quote | ReaderPrefix::Quasiquote)
    ) || calls_head(view, "quote")
}

/// Whether `view` itself is a `(name …)` call.
#[must_use]
pub fn calls_head(view: &ExpressionView, name: &str) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| symbol_is(head, name))
}

/// Whether any form under `view` is a `(head …)` call to one of `heads`.
#[must_use]
pub fn calls_any(view: &ExpressionView, heads: &[&str]) -> bool {
    if is_paren_list(view) && list_head(view).is_some_and(|head| symbol_in(head, heads)) {
        return true;
    }
    view.children.iter().any(|child| calls_any(child, heads))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::view_query::for_each_subview;

    fn parse(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input")
    }

    /// The span of the first `(head …)` list anywhere in the document, which is
    /// what the engine would hand a rule keyed on `head`.
    fn span_of_first(tree: &SyntaxTree, head: &str) -> ByteSpan {
        fn search(view: &ExpressionView, head: &str) -> Option<ByteSpan> {
            if list_head(view).is_some_and(|found| symbol_is(found, head)) {
                return Some(view.span);
            }
            view.children.iter().find_map(|child| search(child, head))
        }
        search(&tree.root_view(), head).expect("a matching list")
    }

    fn site(input: &str, head: &str) -> FormSite {
        let tree = parse(input);
        locate(&tree, span_of_first(&tree, head)).expect("the span belongs to the tree")
    }

    #[test]
    fn a_top_level_definition_is_unquoted_and_top_level() {
        let found = site("(defclass point () (x))", "defclass");
        assert!(!found.quoted);
        assert!(found.top_level);
        assert_eq!(found.top_level_index, 0);
    }

    #[test]
    fn a_quoted_top_level_form_is_data() {
        assert!(site("'(defclass point () (x))", "defclass").quoted);
    }

    #[test]
    fn a_form_nested_inside_quoted_data_is_data() {
        assert!(site("(list '(a (defclass point () (x))))", "defclass").quoted);
    }

    #[test]
    fn a_spelled_out_quote_hides_its_argument_just_like_the_prefix() {
        assert!(site("(quote (defclass point () (x)))", "defclass").quoted);
    }

    #[test]
    fn a_quasiquote_without_an_unquote_is_data() {
        assert!(site("`(defclass point () (x))", "defclass").quoted);
    }

    #[test]
    fn an_unquote_escapes_back_to_code() {
        assert!(!site("`(a ,(defclass point () (x)))", "defclass").quoted);
    }

    #[test]
    fn a_function_quote_is_not_data() {
        assert!(!site("(mapcar #'car (defclass point () (x)))", "defclass").quoted);
    }

    /// The whole of what wrapping does, in one place, because a single quote
    /// *depth* gets the fourth row wrong: it counts the `,` in `'(a ,x)` as an
    /// escape and calls a list literal code.
    ///
    /// A comma escapes back to code only when a backquote opened one. Under a
    /// hard `'` there is no backquote, so `'(a ,x)` is data — literal list
    /// structure in Emacs Lisp, a reader error in SBCL, code in neither.
    #[test]
    fn the_five_wrappings_of_one_form_read_as_code_or_data() {
        let inner = "(defclass point () (x))";
        let cases = [
            (inner.to_owned(), false, "bare code"),
            (format!("'{inner}"), true, "hard quote"),
            (format!("(quote {inner})"), true, "spelled-out quote"),
            (format!("'(a ,{inner})"), true, "comma under a hard quote"),
            (format!("`(a ,{inner})"), false, "comma under a backquote"),
        ];
        for (source, expected, label) in cases {
            assert_eq!(
                site(&source, "defclass").quoted,
                expected,
                "{label}: {source}"
            );
        }
    }

    /// The same five rows through the other reader of quote state, which
    /// `top_level_forms` uses to decide what to correlate against.
    #[test]
    fn top_level_forms_keeps_exactly_the_wrappings_that_are_code() {
        let inner = "(defclass point () (x))";
        for (source, expected) in [
            (inner.to_owned(), 1),
            (format!("'{inner}"), 0),
            (format!("(quote {inner})"), 1), // the `quote` form itself is code
            (format!("'(a ,{inner})"), 0),
            (format!("`(a ,{inner})"), 0), // the backquoted list is data
        ] {
            let tree = parse(&source);
            assert_eq!(
                top_level_forms(&tree).count(),
                expected,
                "for {source}: a data top-level form must not be correlated against"
            );
        }
    }

    #[test]
    fn a_hard_quote_inside_a_backquote_still_stops_a_comma() {
        assert!(site("`(a ,(f '(b ,(defclass point () (x)))))", "defclass").quoted);
    }

    #[test]
    fn two_backquotes_need_two_commas_to_reach_code() {
        assert!(site("``(a ,(defclass point () (x)))", "defclass").quoted);
        assert!(!site("``(a ,,(defclass point () (x)))", "defclass").quoted);
    }

    fn writes(input: &str, head: &str) -> bool {
        let tree = parse(input);
        is_write_position(&tree, span_of_first(&tree, head)).expect("the span belongs to the tree")
    }

    #[test]
    fn a_setf_place_is_a_write_and_the_value_beside_it_is_not() {
        assert!(writes(
            "(setf (slot-value b 'contents) items)",
            "slot-value"
        ));
        assert!(!writes("(setf x (slot-value b 'contents))", "slot-value"));
    }

    #[test]
    fn the_read_modify_write_macros_write_their_place() {
        for source in [
            "(incf (slot-value b 'n))",
            "(decf (slot-value b 'n) 2)",
            "(pop (slot-value b 'items))",
            "(push item (slot-value b 'items))",
            "(pushnew item (slot-value b 'items))",
            "(rotatef (slot-value a 'x) (slot-value b 'x))",
        ] {
            assert!(writes(source, "slot-value"), "for {source}");
        }
    }

    #[test]
    fn a_psetf_writes_only_its_odd_operands() {
        assert!(writes("(psetf (slot-value b 'x) 1)", "slot-value"));
        assert!(!writes("(psetf y 1 z (slot-value b 'x))", "slot-value"));
    }

    /// Only the *immediate* parent decides: the outer place is written, and the
    /// inner call is how the object it writes into is found, so it is a read.
    #[test]
    fn the_object_argument_of_a_nested_place_is_read_not_written() {
        let tree = parse("(setf (slot-value (slot-value a 'inner) 'x) v)");
        let mut spans = Vec::new();
        for_each_subview(&tree.root_view(), |view| {
            if list_head(view).is_some_and(|head| symbol_is(head, "slot-value")) {
                spans.push(view.span);
            }
        });
        assert_eq!(spans.len(), 2, "an outer place and an inner object read");
        spans.sort_unstable_by_key(|span| span.start().get());
        assert_eq!(is_write_position(&tree, spans[0]), Some(true), "the place");
        assert_eq!(
            is_write_position(&tree, spans[1]),
            Some(false),
            "the object read that finds what to write into"
        );
    }

    #[test]
    fn an_ordinary_call_argument_is_not_a_write() {
        for source in [
            "(print (slot-value b 'x))",
            "(slot-value b 'x)",
            "(push (slot-value b 'x) items)",
        ] {
            assert!(!writes(source, "slot-value"), "for {source}");
        }
    }

    #[test]
    fn a_nested_definition_is_located_but_not_top_level() {
        let found = site(
            "(defun f ())\n(eval-when (:load-toplevel)\n  (defclass point () (x)))",
            "defclass",
        );
        assert!(!found.quoted);
        assert!(!found.top_level);
        assert_eq!(found.top_level_index, 1);
    }

    #[test]
    fn top_level_forms_lists_heads_in_file_order() {
        let tree = parse("(defclass a () ())\n(defun f ())\n(defgeneric g (x))\n");
        let heads: Vec<&str> = top_level_forms(&tree).map(|form| form.head).collect();
        assert_eq!(heads, vec!["defclass", "defun", "defgeneric"]);
    }

    #[test]
    fn top_level_forms_drops_a_quoted_form() {
        let tree = parse("'(defclass a () ())\n(defclass b () ())\n");
        let heads: Vec<&str> = top_level_forms(&tree).map(|form| form.head).collect();
        assert_eq!(heads, vec!["defclass"]);
        let indexes: Vec<usize> = top_level_forms(&tree).map(|form| form.index).collect();
        assert_eq!(indexes, vec![1], "the surviving form keeps its own index");
    }

    #[test]
    fn class_form_reads_the_name_supers_and_slots() {
        let tree = parse("(defclass circle (shape drawable) (radius (center :initarg :center)))");
        let view = top_level_view(&tree, 0).expect("the form");
        let class = class_form(&view).expect("a defclass");
        assert_eq!(class.name, "circle");
        assert_eq!(class.superclasses, vec!["shape", "drawable"]);
        assert_eq!(
            class.slots.iter().map(|slot| slot.name).collect::<Vec<_>>(),
            vec!["radius", "center"]
        );
        assert!(class.slots[1].has_option(":initarg"));
        assert!(!class.slots[0].has_option(":initarg"));
    }

    #[test]
    fn a_slot_option_value_is_never_read_as_an_option() {
        let tree = parse("(defclass c () ((x :allocation :class)))");
        let view = top_level_view(&tree, 0).expect("the form");
        let class = class_form(&view).expect("a defclass");
        assert_eq!(class.slots[0].allocation(), ":class");
        assert!(
            !class.slots[0].has_option(":class"),
            "the value of :allocation is not itself an option"
        );
    }

    #[test]
    fn a_slot_with_no_allocation_defaults_to_instance() {
        let tree = parse("(defclass c () ((x :initform 0)))");
        let view = top_level_view(&tree, 0).expect("the form");
        assert_eq!(
            class_form(&view).expect("a defclass").slots[0].allocation(),
            ":instance"
        );
    }

    #[test]
    fn reader_names_collects_readers_and_accessors_only() {
        let tree = parse("(defclass c () ((x :reader x-of :writer set-x :accessor x-at)))");
        let view = top_level_view(&tree, 0).expect("the form");
        let class = class_form(&view).expect("a defclass");
        assert_eq!(class.slots[0].reader_names(), vec!["x-of", "x-at"]);
    }

    #[test]
    fn method_form_splits_qualifiers_from_the_lambda_list() {
        let tree = parse("(defmethod draw :around ((s circle) stream) (call-next-method))");
        let view = top_level_view(&tree, 0).expect("the form");
        let method = method_form(&view).expect("a defmethod");
        assert_eq!(method.name, Some("draw"));
        assert_eq!(
            method
                .qualifiers
                .iter()
                .filter_map(|view| atom_text(view))
                .collect::<Vec<_>>(),
            vec![":around"]
        );
        assert_eq!(method.body.len(), 1);
    }

    #[test]
    fn a_method_with_no_qualifier_has_an_empty_qualifier_list() {
        let tree = parse("(defmethod draw ((s circle)) s)");
        let view = top_level_view(&tree, 0).expect("the form");
        let method = method_form(&view).expect("a defmethod");
        assert!(method.qualifiers.is_empty());
        assert!(method.lambda_list.is_some());
    }

    #[test]
    fn a_setf_method_name_is_a_list_not_a_symbol() {
        let tree = parse("(defmethod (setf width) (value (s circle)) value)");
        let view = top_level_view(&tree, 0).expect("the form");
        let method = method_form(&view).expect("a defmethod");
        assert_eq!(method.name, None);
        assert!(method.qualifiers.is_empty(), "the name is not a qualifier");
    }

    #[test]
    fn a_signature_normalizes_an_unspecialized_parameter_to_t() {
        let tree = parse("(defmethod draw ((s circle) stream) s)");
        let signature = method_signature(&tree, 0).expect("a defmethod");
        assert_eq!(signature.name, "draw");
        assert!(signature.qualifiers.is_empty());
        assert_eq!(signature.specializers, vec!["circle", "t"]);
    }

    #[test]
    fn a_signature_stops_at_the_first_lambda_list_keyword() {
        let tree = parse("(defmethod draw ((s circle) &optional (n 1) &key k) s)");
        let signature = method_signature(&tree, 0).expect("a defmethod");
        assert_eq!(signature.specializers, vec!["circle"]);
    }

    #[test]
    fn a_signature_folds_case_and_carries_its_qualifiers() {
        let tree = parse("(DEFMETHOD Draw :AROUND ((s Circle)) s)");
        let signature = method_signature(&tree, 0).expect("a defmethod");
        assert_eq!(signature.name, "draw");
        assert_eq!(signature.qualifiers, vec![":around"]);
        assert_eq!(signature.specializers, vec!["circle"]);
    }

    #[test]
    fn a_name_is_normalized_the_same_way_wherever_it_is_read() {
        let tree = parse("(defgeneric CL-USER:draw (x))\n(defmethod draw ((x t)) x)\n");
        let generic = top_level_view(&tree, 0).expect("the defgeneric");
        assert_eq!(definition_name(&generic), Some("draw".to_owned()));
        assert_eq!(
            method_signature(&tree, 1).expect("a defmethod").name,
            "draw"
        );
    }

    #[test]
    fn a_setf_name_strips_only_its_own_package_qualification() {
        let tree = parse("(defmethod (setf cl-user:width) (v (s circle)) v)");
        let view = top_level_view(&tree, 0).expect("the form");
        assert_eq!(definition_name(&view), Some("(setf width)".to_owned()));
        assert_eq!(
            method_signature(&tree, 0).expect("a defmethod").name,
            "(setf width)"
        );
    }

    #[test]
    fn an_eql_specializer_keeps_its_whole_form() {
        let tree = parse("(defmethod area ((n (eql 0))) 0)");
        let signature = method_signature(&tree, 0).expect("a defmethod");
        assert_eq!(signature.specializers, vec!["(eql 0)"]);
    }

    #[test]
    fn a_quoted_symbol_is_read_through_either_spelling() {
        for source in ["(slot-value o 'radius)", "(slot-value o (quote radius))"] {
            let tree = parse(source);
            let view = top_level_view(&tree, 0).expect("the form");
            assert_eq!(
                quoted_symbol(&view.children[2]),
                Some("radius"),
                "for {source}"
            );
        }
    }

    #[test]
    fn an_unquoted_or_function_quoted_argument_names_no_slot() {
        for source in ["(slot-value o name)", "(slot-value o #'radius)"] {
            let tree = parse(source);
            let view = top_level_view(&tree, 0).expect("the form");
            assert_eq!(quoted_symbol(&view.children[2]), None, "for {source}");
        }
    }

    #[test]
    fn mentions_finds_a_bare_symbol_and_calls_any_does_not() {
        let tree = parse("(defmethod f ((x t)) (apply #'call-next-method nil))");
        let view = top_level_view(&tree, 0).expect("the form");
        assert!(mentions(&view, &["call-next-method"]));
        assert!(!calls_any(&view, &["call-next-method"]));
        assert!(calls_any(&view, &["apply"]));
    }
}
