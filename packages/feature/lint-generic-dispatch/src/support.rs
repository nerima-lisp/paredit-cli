//! What the dispatch-protocol rules share: evaluation context, lambda-list
//! shape, and `defmethod` geometry.
//!
//! Three things, each of them forced by a constraint the engine imposes.
//!
//! - **Evaluation context.** [`is_unevaluated_at`] answers "is this code or is
//!   it a list literal", which a head-matched node cannot answer about itself:
//!   [`RuleContext`] carries no parent pointer, and the dispatcher walks into
//!   `'(…)` like any other subtree. It answers without ever calling
//!   [`SyntaxTree::root_view`], which materializes the whole document — see its
//!   own documentation for the 4.49-second measurement that forced that.
//! - **Lambda-list shape.** [`LambdaList`] is the whole of what CLHS 7.6.4
//!   compares, and nothing else: required and optional *counts*, whether
//!   `&rest`/`&key` are present at all, and which keyword names are accepted.
//! - **`defmethod` geometry.** The qualifiers sit *between* the name and the
//!   lambda list, so a `defmethod` with `:around` puts its lambda list one child
//!   further along than one without. [`method_parts`] finds it by shape rather
//!   than by index; a rule that counted to a fixed index would be wrong on every
//!   qualified method, which is a bug a sibling package had to fix in PR #90.
//!
//! ## Reader conditionals
//!
//! Under this repository's dialect-aware parse, `#+sbcl (form)` is a **single
//! atom** and `atom_text` returns its text with the `#+` still attached. So a
//! reader conditional anywhere among a form's children shifts every later index
//! and hides a whole subform. [`has_reader_conditional`] declines such a form
//! outright rather than guessing, which is the same call
//! `paredit-feature-lint-type-declaration` made after its own corpus audit found
//! the shape in SBCL's sources.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

/// Every rule in this package models the CLOS generic-function protocol, which
/// exists in Common Lisp and nowhere else this tool reads.
///
/// Stated rather than left to the [`RuleDialectScope`] trait default, which is
/// also Common Lisp: a rule that *defaulted* to it and one that *declares* it
/// are indistinguishable at the call site, and this package's engine tests
/// assert the declaration.
pub const COMMON_LISP_ONLY: RuleDialectScope = RuleDialectScope::new(&[Dialect::CommonLisp]);

/// The three CLOS definition heads this package anchors on.
///
/// Kept as one constant so the head index and the cost tests' control rule
/// cannot drift apart from what the rules actually declare.
pub const DISPATCH_HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defgeneric"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defclass"),
];

// -- evaluation context -------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single depth counter gets `'(a ,x)` wrong in the direction that
/// produces false positives.
///
/// This is deliberately the same model — down to the field names — that
/// `paredit-feature-lint-type-declaration` and
/// `paredit-feature-lint-object-system` each carry. The packages hold their own
/// copies rather than a dependency between sibling features, because
/// `tests/cli/feature_dependency_contract.rs` scans the whole manifest text for
/// `paredit-feature-`; consolidating the several quote walks in the tree is its
/// own ticket.
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

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, `#+` and the rest are deliberately neutral: none of them
    /// turns code into data.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which hand-written code and macro output both
/// spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The one child of `view` whose span contains `target`, by **binary search**.
///
/// Children are in source order and their spans do not overlap, so the first
/// child whose span ends after `target` starts is the only one that can contain
/// it. This has to be a search rather than a scan: the first level of the
/// descent is the file's *root*, whose children are its top-level forms, and a
/// linear `find` there costs one pass over every top-level form **per finding**.
fn containing_child(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let index = view
        .children
        .partition_point(|child| child.span.end().get() <= target.start().get());
    view.children
        .get(index)
        .filter(|child| span_contains(child.span, target))
}

/// Whether `view` — a node the dispatcher matched — is unevaluated data rather
/// than code.
///
/// # Why this never touches [`SyntaxTree::root_view`]
///
/// The obvious implementation descends from `tree.root_view()`, and that call
/// **materializes the whole document** as a tree of [`ExpressionView`]s. Once
/// per *finding*, on a file where every form reports, that is a per-finding cost
/// proportional to the file — a rule that is linear while declining and
/// quadratic while reporting, which no correctness test can see.
///
/// It is not hypothetical. With the `root_view()` descent, this package's
/// `defgeneric-method-option-incongruent` measured **4.49 s at 2000 reporting
/// forms**, 2.25 ms per invocation, at an 8x doubling ratio of 55 where linear
/// is 8 — on an analysis that is entirely local to one form. Every nanosecond of
/// it was here.
///
/// So the answer is computed in two steps, neither of which materializes
/// anything the rule does not already hold:
///
/// 1. **The node's own reader prefixes**, read off the `view` the dispatcher
///    already handed the rule. A `'` or `` ` `` here settles it outright.
/// 2. **Whether the node is a top-level form**, by binary search over
///    [`SyntaxTree::root_child_span`] — `log2(forms)` slice indexes, no
///    allocation, no view. A top-level form has no ancestors, so step 1 was the
///    whole answer for it, and that is the case every rule here is in almost
///    always.
///
/// Only a *nested* match reaches the descent, and even then it materializes the
/// **one enclosing top-level form** rather than the document.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, view: &ExpressionView) -> bool {
    // (1) The node's own spelling. Sufficient on its own to say "data".
    if QuoteState::EVALUATED.after_prefixes(view).is_data() {
        return true;
    }
    let target = view.span;
    // (2) Which top-level form holds it, and whether it *is* that form.
    let Some(index) = top_level_index_containing(tree, target) else {
        return false;
    };
    if tree.root_child_span(index) == Some(target) {
        // A top-level form has no ancestor that could quote it, and step 1
        // already read its own prefixes.
        return false;
    }
    // (3) The descent, within one top-level form.
    let Ok(selection) = tree.select_path(&SexprPath::root_child(index)) else {
        return false;
    };
    let form = selection.view();
    let mut state = QuoteState::EVALUATED.after_prefixes(&form);
    let mut view: &ExpressionView = &form;
    loop {
        let quoting = is_quote_form(view);
        let Some(child) = containing_child(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

/// The index of the top-level form whose span contains `target`, by binary
/// search over [`SyntaxTree::root_child_span`].
///
/// `root_child_span` is the allocation-free counterpart of
/// `select_path(&Path::root_child(index))?.span()`; the latter heap-allocates a
/// `Vec<ChildIndex>` per call, and a binary search would pay `log2(forms)` of
/// them to answer one question about one node.
fn top_level_index_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    let mut low = 0usize;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        let span = tree.root_child_span(middle)?;
        if span.end().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let span = tree.root_child_span(low)?;
    // Totality, not a guard. `target` always comes from a node the dispatcher
    // matched in *this* tree, so some top-level form contains it and this check
    // cannot fail — mutation-testing it to `Some(low)` kills no test, and the
    // input that would kill one is a span from a different document. It is kept
    // because without it the function's contract would silently become "the
    // nearest top-level form" for a caller that ever passes a foreign span,
    // which is the same shape as `RuleContext::slice`'s unreachable fallback.
    span_contains(span, target).then_some(low)
}

// -- symbols ------------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of its
/// package qualifier — the spelling every comparison here is written in.
///
/// The reader upcases unescaped symbols and a package prefix does not change
/// which function is named, so `CL-USER:Draw` and `draw` are one name.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// Whether this node is a form the reader folded away behind `#+` or `#-`.
///
/// Under the dialect-aware parse a reader conditional and the form it guards are
/// one atom whose text begins `#+`. Guards written on the assumption that
/// `#+sbcl` is a separate node are simply unreachable.
#[must_use]
pub fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether any child of `view` is a folded reader conditional, which shifts
/// every later index and hides a whole subform.
///
/// Every rule here declines such a form rather than guessing at the geometry.
#[must_use]
pub fn has_reader_conditional(view: &ExpressionView) -> bool {
    view.children.iter().any(is_reader_conditional)
}

/// Whether any form under `view` calls one of `names`, or names one of them as a
/// bare symbol.
///
/// The bare-symbol case is deliberate: `(apply #'call-next-method args)` reaches
/// `call-next-method` just as much as a direct call does, so a rule asking
/// "does this body ever reach it" must answer yes. An occurrence inside quoted
/// data also counts, which costs a finding and never invents one.
#[must_use]
pub fn mentions(view: &ExpressionView, names: &[&str]) -> bool {
    if atom_symbol_text(view).is_some_and(|text| {
        let normalized = normalized_symbol(text);
        names.contains(&normalized.as_str())
    }) {
        return true;
    }
    view.children.iter().any(|child| mentions(child, names))
}

// -- lambda lists -------------------------------------------------------------

/// The whole of what CLHS 7.6.4 compares between a generic function's lambda
/// list and a method's.
///
/// Deliberately not a parse of the lambda list: nothing here records a parameter
/// *name*, because congruence never looks at one. What it looks at is verified
/// against SBCL 2.6.0, one case at a time — see the module's tests and the
/// rule's documentation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LambdaList {
    /// Required parameters. A specialized one, `(s circle)`, counts as one.
    pub required: usize,
    /// Parameters after `&optional` and before the next lambda-list keyword.
    pub optional: usize,
    /// Whether `&rest` or `&body` appears.
    pub rest: bool,
    /// Whether `&key` appears.
    pub key: bool,
    /// Whether `&allow-other-keys` appears.
    pub allow_other_keys: bool,
    /// The keyword names accepted after `&key`, normalized with a leading
    /// colon: `b` and `((:b var) 0)` both yield `":b"`.
    pub keywords: Vec<String>,
}

impl LambdaList {
    /// Whether this lambda list accepts *any* keyword argument, which is what
    /// releases a method from having to name the generic's keywords.
    ///
    /// Verified against SBCL 2.6.0: with `(defgeneric k4 (a &key b))`, both
    /// `(defmethod k4 ((a t) &rest r) …)` and
    /// `(defmethod k4 ((a t) &key &allow-other-keys) …)` are accepted, while
    /// `(defmethod g4 ((a t) &key) …)` against `(defgeneric g4 (a &key b))`
    /// signals `SIMPLE-PROGRAM-ERROR`.
    #[must_use]
    pub const fn accepts_any_keyword(&self) -> bool {
        self.allow_other_keys || (self.rest && !self.key)
    }

    /// Whether `&rest` or `&key` is present at all.
    ///
    /// CLHS 7.6.4 treats the two as one question: a generic and a method must
    /// agree on whether they accept *some* trailing arguments, but not on which
    /// of the two spellings says so. Verified: `(defgeneric g5 (a &rest r))`
    /// accepts `(defmethod g5 ((a t) &key k) …)`, while `(defgeneric k1 (a))`
    /// rejects `(defmethod k1 ((a t) &key opt) …)` with "the method and generic
    /// function differ in whether they accept &REST or &KEY arguments".
    #[must_use]
    pub const fn accepts_trailing(&self) -> bool {
        self.rest || self.key
    }
}

/// Which section of a lambda list the walk is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Required,
    Optional,
    Key,
    /// `&aux`, `&environment`, `&whole` and anything else: never part of
    /// congruence, so the walk stops counting.
    Ignored,
}

/// Reads a lambda list's shape, or `None` when it is not a `(…)` list or carries
/// a folded reader conditional.
#[must_use]
pub fn lambda_list_of(view: &ExpressionView) -> Option<LambdaList> {
    if !is_paren_list(view) || has_reader_conditional(view) {
        return None;
    }
    let mut shape = LambdaList::default();
    let mut section = Section::Required;

    for child in &view.children {
        let keyword = symbol_name(child).filter(|name| name.starts_with('&'));
        if let Some(keyword) = keyword {
            section = match keyword.as_str() {
                "&optional" => Section::Optional,
                "&rest" | "&body" => {
                    shape.rest = true;
                    Section::Ignored
                }
                "&key" => {
                    shape.key = true;
                    Section::Key
                }
                "&allow-other-keys" => {
                    shape.allow_other_keys = true;
                    Section::Ignored
                }
                _ => Section::Ignored,
            };
            continue;
        }
        match section {
            Section::Required => shape.required += 1,
            Section::Optional => shape.optional += 1,
            Section::Key => {
                if let Some(name) = keyword_name_of(child) {
                    shape.keywords.push(name);
                }
            }
            Section::Ignored => {}
        }
    }
    Some(shape)
}

/// The keyword name one `&key` parameter accepts.
///
/// Three spellings, all of CLHS 3.4.1: `b` accepts `:b`; `(b 0)` and
/// `(b 0 b-p)` accept `:b`; `((:other b) 0)` accepts `:other` and says nothing
/// about `:b`.
fn keyword_name_of(view: &ExpressionView) -> Option<String> {
    if let Some(name) = symbol_name(view) {
        return Some(format!(":{}", name.trim_start_matches(':')));
    }
    let first = view.children.first()?;
    if let Some(name) = symbol_name(first) {
        return Some(format!(":{}", name.trim_start_matches(':')));
    }
    // `((:other b) 0)`: the keyword is spelled out inside a nested list.
    let spelled = first.children.first().and_then(symbol_name)?;
    Some(format!(":{}", spelled.trim_start_matches(':')))
}

// -- defmethod geometry -------------------------------------------------------

/// A `(defmethod name qualifier* specialized-lambda-list . body)` form, split at
/// the place CLOS splits it.
#[derive(Debug, Clone, Copy)]
pub struct MethodParts<'a> {
    /// The name node — an atom, or the `(setf …)` list.
    pub name: &'a ExpressionView,
    /// The atoms between the name and the lambda list.
    pub qualifiers: &'a [ExpressionView],
    pub lambda_list: &'a ExpressionView,
    pub body: &'a [ExpressionView],
}

impl MethodParts<'_> {
    /// Whether this is a **primary** method: no qualifier at all.
    ///
    /// The distinction the whole `call-next-method` question turns on. A
    /// primary method *is* the effective method's centre; an `:after` method is
    /// not required to call anything, and an `:around` that short-circuits is
    /// `around-method-missing-call-next-method`'s subject in
    /// `paredit-feature-lint-object-system`, not this package's.
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.qualifiers.is_empty()
    }

    /// The generic function's name in the normalized spelling, or `None` when it
    /// is a `(setf …)` list.
    #[must_use]
    pub fn generic_name(&self) -> Option<String> {
        symbol_name(self.name)
    }
}

/// Splits a `defmethod` form, or `None` when it has no lambda list or carries a
/// folded reader conditional.
///
/// The lambda list is the first `(…)` at or after index 2, which is exactly how
/// CLOS separates it from the qualifiers: qualifiers are non-list objects, and
/// there is no other way to tell `:around` from a lambda list. Counting to a
/// fixed index instead is wrong on every qualified method.
#[must_use]
pub fn method_parts(view: &ExpressionView) -> Option<MethodParts<'_>> {
    if has_reader_conditional(view) {
        return None;
    }
    let name = view.children.get(1)?;
    let index = view
        .children
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, child)| child.kind != ExpressionKind::Atom)
        .map(|(index, _)| index)?;
    Some(MethodParts {
        name,
        qualifiers: &view.children[2..index],
        lambda_list: &view.children[index],
        body: &view.children[index + 1..],
    })
}

/// A `(:method qualifier* specialized-lambda-list . body)` option of a
/// `defgeneric`, read with the same geometry a `defmethod` has.
///
/// `(:method …)` is a `defmethod` with the head replaced, so the split is the
/// same one shifted by nothing: `(:method :around ((s c)) …)` puts its lambda
/// list at the same index `(defmethod f :around ((s c)) …)` does, because
/// `:method` stands where `f` stands.
///
/// There is deliberately **no reader-conditional guard here**, unlike in
/// [`method_parts`], and the asymmetry is the point. `method_parts` is read by a
/// rule whose finding depends on the method's *body*, and a folded `#+sbcl
/// (call-next-method)` atom hides a call that rule has to see. The only rule
/// reading a `(:method …)` option judges its **lambda list**, and a folded atom
/// is not a list — so it is declined by the shape check below and can never be
/// mistaken for one. Guarding here killed no test and did cost real findings:
/// `(defgeneric g (a) (:method ((a t) (b t)) #+sbcl (foo)))` is incongruent
/// under every reading of the conditional, and declining it loses that.
#[must_use]
pub fn method_option_parts(view: &ExpressionView) -> Option<MethodParts<'_>> {
    let index = view
        .children
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, child)| child.kind != ExpressionKind::Atom)
        .map(|(index, _)| index)?;
    Some(MethodParts {
        // A `(:method …)` option names no generic of its own; the head stands
        // in so that `MethodParts` stays one type.
        name: &view.children[0],
        qualifiers: &view.children[1..index],
        lambda_list: &view.children[index],
        body: &view.children[index + 1..],
    })
}

/// Whether `view` is a `(:method …)` option: a `(…)` list whose head is the
/// keyword `:method`.
#[must_use]
pub fn is_method_option(view: &ExpressionView) -> bool {
    is_paren_list(view) && view.children.first().and_then(symbol_name).as_deref() == Some(":method")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    /// The tree, and the first top-level form's view — which is what the
    /// dispatcher hands a rule.
    fn first_form(source: &str) -> (SyntaxTree, ExpressionView) {
        let parsed = tree(source);
        let view = parsed.root_view().children[0].clone();
        (parsed, view)
    }

    /// Whether the first `(head …)` list anywhere in the document reads as data.
    fn nested_is_data(source: &str, head: &str) -> bool {
        fn search(view: &ExpressionView, head: &str) -> Option<ExpressionView> {
            if list_head(view).is_some_and(|found| symbol_is(found, head)) {
                return Some(view.clone());
            }
            view.children.iter().find_map(|child| search(child, head))
        }
        let parsed = tree(source);
        let found = search(&parsed.root_view(), head).expect("a matching list");
        is_unevaluated_at(&parsed, &found)
    }

    // -- evaluation context --------------------------------------------------

    #[test]
    fn a_definition_in_plain_code_reads_as_evaluated() {
        let (parsed, view) = first_form("(defmethod draw ((s circle)) s)");
        assert!(!is_unevaluated_at(&parsed, &view));
    }

    #[test]
    fn a_definition_inside_a_quote_reads_as_data() {
        let (parsed, view) = first_form("'(defmethod draw ((s circle)) s)");
        assert!(is_unevaluated_at(&parsed, &view));
    }

    #[test]
    fn a_definition_inside_a_backquote_reads_as_data() {
        let (parsed, view) = first_form("`(defmethod draw ((s circle)) s)");
        assert!(is_unevaluated_at(&parsed, &view));
    }

    /// The two-counter model's reason for existing: a comma inside a hard quote
    /// is a comma character, not an escape back to code.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(nested_is_data(
            "'(a ,(defmethod draw ((s circle)) s))",
            "defmethod"
        ));
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert!(!nested_is_data(
            "`(a ,(defmethod draw ((s circle)) s))",
            "defmethod"
        ));
    }

    /// The nested descent, which is the only path that materializes anything:
    /// a `defmethod` inside a quoted list is data, and one inside an
    /// `eval-when` beside it is not.
    #[test]
    fn a_nested_form_is_judged_by_its_ancestors() {
        assert!(nested_is_data(
            "(list 1 2)\n(list '(a (defmethod draw ((s circle)) s)))",
            "defmethod"
        ));
        assert!(!nested_is_data(
            "(list 1 2)\n(eval-when (:load-toplevel) (defmethod draw ((s circle)) s))",
            "defmethod"
        ));
    }

    /// A spelled-out `(quote …)` quotes its argument exactly as `'` does, and
    /// only the descent can see it — the node's own prefixes are empty.
    #[test]
    fn a_spelled_out_quote_is_seen_by_the_descent() {
        assert!(nested_is_data(
            "(quote (defmethod draw ((s circle)) s))",
            "defmethod"
        ));
    }

    /// The binary search must find the right top-level form, not merely *a*
    /// form: with many forms before it, a mis-ordered search would judge the
    /// target against a neighbour's quoting.
    #[test]
    fn the_top_level_search_finds_the_form_that_holds_the_target() {
        let mut source = String::new();
        for index in 0..64 {
            source.push_str(&format!("'(quoted {index})\n"));
        }
        source.push_str("(eval-when (:load-toplevel) (defmethod draw ((s circle)) s))\n");
        for index in 0..64 {
            source.push_str(&format!("'(more {index})\n"));
        }
        assert!(
            !nested_is_data(&source, "defmethod"),
            "the target sits in the one unquoted form among 128 quoted ones"
        );
    }

    // -- lambda lists --------------------------------------------------------

    fn shape(source: &str) -> LambdaList {
        let parsed = tree(source);
        lambda_list_of(&parsed.root_view().children[0]).expect("a lambda list")
    }

    #[test]
    fn a_plain_lambda_list_counts_its_required_parameters() {
        let found = shape("(a b c)");
        assert_eq!(found.required, 3);
        assert_eq!(found.optional, 0);
        assert!(!found.accepts_trailing());
    }

    #[test]
    fn a_specialized_parameter_counts_as_one_required_parameter() {
        let found = shape("((s circle) stream)");
        assert_eq!(found.required, 2);
    }

    #[test]
    fn optionals_are_counted_and_their_defaults_are_not() {
        let found = shape("(a &optional b (c 0) (d 0 d-p))");
        assert_eq!(found.required, 1);
        assert_eq!(found.optional, 3);
    }

    #[test]
    fn keywords_are_read_in_all_three_spellings() {
        let found = shape("(a &key b (c 0) (d 0 d-p) ((:other e) 1))");
        assert_eq!(found.keywords, vec![":b", ":c", ":d", ":other"]);
        assert!(found.key);
        assert!(!found.allow_other_keys);
    }

    #[test]
    fn rest_and_body_both_set_the_rest_flag() {
        assert!(shape("(a &rest more)").rest);
        assert!(shape("(a &body more)").rest);
    }

    #[test]
    fn aux_parameters_are_not_counted_anywhere() {
        let found = shape("(a &aux (b 1) (c 2))");
        assert_eq!(found.required, 1);
        assert_eq!(found.optional, 0);
        assert!(found.keywords.is_empty());
    }

    /// The two releases from having to name the generic's keywords, exactly as
    /// SBCL 2.6.0 grants them.
    #[test]
    fn accepting_any_keyword_needs_allow_other_keys_or_a_bare_rest() {
        assert!(shape("(a &rest r)").accepts_any_keyword());
        assert!(shape("(a &key &allow-other-keys)").accepts_any_keyword());
        assert!(shape("(a &rest r &key b &allow-other-keys)").accepts_any_keyword());
        assert!(!shape("(a &key b)").accepts_any_keyword());
        // A `&rest` *with* a `&key` accepts only the named keywords.
        assert!(!shape("(a &rest r &key b)").accepts_any_keyword());
    }

    #[test]
    fn a_lambda_list_carrying_a_reader_conditional_is_declined() {
        let parsed = tree("(a #+sbcl b)");
        assert_eq!(lambda_list_of(&parsed.root_view().children[0]), None);
    }

    #[test]
    fn an_atom_is_not_a_lambda_list() {
        let parsed = tree("nil");
        assert_eq!(lambda_list_of(&parsed.root_view().children[0]), None);
    }

    // -- defmethod geometry --------------------------------------------------

    /// The trap: a qualifier displaces the lambda list by one child, so a rule
    /// counting to a fixed index is wrong on every qualified method.
    #[test]
    fn a_qualifier_displaces_the_lambda_list_and_the_split_follows_it() {
        let parsed = tree("(defmethod draw :around ((s circle) stream) (call-next-method))");
        let form = &parsed.root_view().children[0];
        let parts = method_parts(form).expect("a defmethod");
        assert_eq!(parts.generic_name(), Some("draw".to_owned()));
        assert_eq!(parts.qualifiers.len(), 1);
        assert!(!parts.is_primary());
        assert_eq!(
            lambda_list_of(parts.lambda_list).expect("shape").required,
            2
        );
        assert_eq!(parts.body.len(), 1);
    }

    #[test]
    fn an_unqualified_method_is_primary() {
        let parsed = tree("(defmethod draw ((s circle)) s)");
        let form = &parsed.root_view().children[0];
        let parts = method_parts(form).expect("a defmethod");
        assert!(parts.is_primary());
        assert!(parts.qualifiers.is_empty());
    }

    #[test]
    fn a_setf_method_name_is_a_list_and_names_no_symbol() {
        let parsed = tree("(defmethod (setf width) (value (s circle)) value)");
        let form = &parsed.root_view().children[0];
        let parts = method_parts(form).expect("a defmethod");
        assert_eq!(parts.generic_name(), None);
        assert!(
            parts.qualifiers.is_empty(),
            "the (setf …) name is not a qualifier"
        );
    }

    #[test]
    fn a_method_with_no_lambda_list_is_declined() {
        let parsed = tree("(defmethod draw)");
        assert_eq!(
            method_parts(&parsed.root_view().children[0]).map(|parts| parts.qualifiers.len()),
            None
        );
    }

    #[test]
    fn a_method_option_splits_the_same_way_a_defmethod_does() {
        let parsed = tree("(defgeneric g (a) (:method :around ((a t)) (call-next-method)))");
        let form = &parsed.root_view().children[0];
        let option = &form.children[3];
        assert!(is_method_option(option));
        let parts = method_option_parts(option).expect("a :method option");
        assert_eq!(parts.qualifiers.len(), 1);
        assert!(!parts.is_primary());
        assert_eq!(
            lambda_list_of(parts.lambda_list).expect("shape").required,
            1
        );
    }

    #[test]
    fn a_documentation_option_is_not_a_method_option() {
        let parsed = tree("(defgeneric g (a) (:documentation \"doc\"))");
        let form = &parsed.root_view().children[0];
        assert!(!is_method_option(&form.children[3]));
    }

    // -- mentions ------------------------------------------------------------

    #[test]
    fn a_bare_function_designator_counts_as_a_mention() {
        let parsed = tree("(defmethod f ((x t)) (apply #'call-next-method nil))");
        assert!(mentions(
            &parsed.root_view().children[0],
            &["call-next-method"]
        ));
    }

    #[test]
    fn a_package_qualified_mention_still_counts() {
        let parsed = tree("(defmethod f ((x t)) (cl:call-next-method))");
        assert!(mentions(
            &parsed.root_view().children[0],
            &["call-next-method"]
        ));
    }

    #[test]
    fn an_unrelated_body_mentions_nothing() {
        let parsed = tree("(defmethod f ((x t)) (print x))");
        assert!(!mentions(
            &parsed.root_view().children[0],
            &["call-next-method"]
        ));
    }
}
