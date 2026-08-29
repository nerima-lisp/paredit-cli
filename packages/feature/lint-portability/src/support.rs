//! What the portability rules share: whether a matched node is *code*.
//!
//! The lint engine's dispatch walks into quoted data like any other subtree and
//! [`RuleContext`] carries no parent pointer, so a head-matched node cannot
//! tell on its own whether it is a call or a symbol in a literal list.
//! `'(sort (sort xs #'<) #'>)` is three lists of symbols, not two sorts.
//! [`is_unevaluated_at`] answers that.
//!
//! # Quote semantics
//!
//! `QuoteState` is copied from `paredit-feature-lint-condition-system`'s
//! `support.rs` (and its sibling copy in `paredit-feature-lint-testing`),
//! tests included, deliberately as a copy rather than as a dependency: a lint
//! feature package depending on another lint feature package would be a new
//! feature→feature edge for sixty lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A single `i32` depth
//! counter gets `'(a ,X)` wrong, and a node-local `reader_prefixes` check gets
//! `'(outer (inner))` wrong — the inner node carries no prefix of its own and
//! is still data. Both shapes are pinned below.
//!
//! # Cost
//!
//! Nothing here runs per visited node. Every caller invokes
//! [`is_unevaluated_at`] at most once, *after* it already has a finding to
//! report — which, in the `clean/forms/*` benchmarks that lint files with zero
//! findings, is never.
//!
//! [`is_unevaluated_at`] never calls `SyntaxTree::root_view`. That builds an
//! `ExpressionView` — a `Vec` of children and a `Vec` of reader prefixes — for
//! *every node in the file*, so asking it about one node costs the whole
//! document, and a file with N findings would cost N×N. Selecting the one
//! enclosing top-level form instead costs a binary search over the top level —
//! each step a node-id lookup and a span read, neither of which allocates —
//! plus that one form's own subtree.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_is};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing.
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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none
    /// of them turns code into data.
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

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other.
const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let child = view.children.get(after.checked_sub(1)?)?;
    span_contains(child.span, target).then_some(child)
}

/// The top-level form containing `target`, materialized on its own.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
    // Top-level forms are in document order and do not overlap, so the only
    // candidate is the last one beginning at or before `target`.
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if start_of(middle)? <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let selection = tree
        .select_path(&Path::root_child(low.checked_sub(1)?))
        .ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(sort b c)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);

    while view.span != target {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = child_containing(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }
    state.is_data()
}

/// Whether two forms are written the same way, ignoring case and package
/// qualification on symbols.
///
/// Used to decide whether a re-sort supplies a *different* ordering key, which
/// is the difference between a multi-pass sort that needs stability and a
/// redundant one that does not. Compares shape rather than source text so that
/// whitespace and comments between two otherwise identical predicates do not
/// make them look different.
///
/// Reader prefixes are part of the shape: `#'<` and `<` are not the same
/// argument. For an *atom* the comparison of `text` already settles that, since
/// the reader keeps an atom's prefix in its text (`#'<` is one token of text
/// `"#'<"`); for a *list* the text is `None` on both sides and the prefix
/// comparison is the only thing that separates `'(a b)` from `(a b)`. Both
/// cases are pinned below.
#[must_use]
pub fn same_shape(left: &ExpressionView, right: &ExpressionView) -> bool {
    if left.kind != right.kind
        || left.delimiter != right.delimiter
        || left.reader_prefixes != right.reader_prefixes
        || left.children.len() != right.children.len()
    {
        return false;
    }
    match (left.text.as_deref(), right.text.as_deref()) {
        (Some(one), Some(other)) if !one.eq_ignore_ascii_case(other) => return false,
        (Some(_), None) | (None, Some(_)) => return false,
        _ => {}
    }
    left.children
        .iter()
        .zip(right.children.iter())
        .all(|(one, other)| same_shape(one, other))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parse(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn root_form(tree: &SyntaxTree) -> ExpressionView {
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    /// The span of the first `(sort …)` list anywhere in the file, found by an
    /// explicit recursion so the helper does not depend on the very quote logic
    /// under test.
    ///
    /// Searches from the document root rather than from one top-level form, so
    /// that a file whose `sort` is in its *second* form is still found — the
    /// shape that catches a `root_child_containing` which always answers about
    /// form zero.
    fn first_sort_span(view: &ExpressionView) -> Option<ByteSpan> {
        if list_head(view).is_some_and(|head| symbol_is(head, "sort")) {
            return Some(view.span);
        }
        view.children.iter().find_map(first_sort_span)
    }

    fn sort_is_data(source: &str) -> bool {
        let tree = parse(source);
        let span = first_sort_span(&tree.root_view()).expect("a (sort …) list");
        is_unevaluated_at(&tree, span)
    }

    // -- the five quote shapes every rule in this package depends on ---------

    #[test]
    fn plain_code_is_evaluated() {
        assert!(!sort_is_data("(sort xs #'<)"));
    }

    #[test]
    fn a_quoted_list_is_data() {
        assert!(sort_is_data("'(sort xs #'<)"));
    }

    #[test]
    fn a_long_hand_quote_form_makes_its_argument_data() {
        assert!(sort_is_data("(quote (sort xs #'<))"));
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(sort_is_data("`(sort xs #'<)"));
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert!(!sort_is_data("`(a ,(sort xs #'<))"));
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(sort_is_data("'(a ,(sort xs #'<))"));
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(sort_is_data("'(outer (sort xs #'<))"));
    }

    #[test]
    fn a_node_in_the_second_top_level_form_is_judged_by_that_form() {
        assert!(sort_is_data("(defun f () nil)\n'(sort xs #'<)"));
        assert!(!sort_is_data("'(nothing here)\n(sort xs #'<)"));
    }

    // -- shape comparison ----------------------------------------------------

    fn shapes_match(left: &str, right: &str) -> bool {
        let one = parse(left);
        let other = parse(right);
        same_shape(&root_form(&one), &root_form(&other))
    }

    #[test]
    fn reads_two_spellings_of_one_predicate_as_the_same() {
        assert!(shapes_match("#'<", "#'<"));
        assert!(shapes_match("#'string<", "#'STRING<"));
        assert!(shapes_match(
            "(lambda (a b) (< a b))",
            "(lambda (a b) (< a b))"
        ));
    }

    #[test]
    fn reads_different_predicates_as_different() {
        assert!(!shapes_match("#'<", "#'>"));
        assert!(!shapes_match("#'<", "<"));
        assert!(!shapes_match("#'car", "#'cdr"));
        // A *list*: both sides have `text == None` and identical children, so
        // the reader-prefix comparison is the only thing that separates them.
        // Without it this pair compares equal — which the mutation harness
        // proved, when the atom cases above did not.
        assert!(!shapes_match("'(a b)", "(a b)"));
        assert!(!shapes_match("`(a b)", "(a b)"));
        assert!(!shapes_match(
            "(lambda (a b) (< a b))",
            "(lambda (a b) (> a b))"
        ));
    }
}
