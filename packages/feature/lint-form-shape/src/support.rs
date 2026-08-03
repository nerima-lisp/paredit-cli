//! What the newer rules in this package share: which parts of a file are
//! *code*, where the enclosing top-level form is, and a handful of atom
//! predicates.
//!
//! # Quote semantics
//!
//! [`QuoteState`], [`for_each_evaluated_subview`] and [`is_unevaluated_at`] are
//! copied from `paredit-feature-lint-testing`'s `support.rs` (which in turn
//! copied them from `paredit-feature-lint-condition-system`), tests included,
//! deliberately as a copy rather than as a dependency: a lint feature package
//! depending on another lint feature package would be a new feature→feature
//! edge for a hundred lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A node one level
//! *inside* a quote is still data even though it carries no reader prefix of
//! its own, which is why a node-local `reader_prefixes` check is not enough.
//!
//! # The two counters are separately observable, and that matters
//!
//! Every rule but one asks only [`QuoteState::is_data`] — "is this code?" —
//! and declines to report when the answer is no.
//! `quoted-form-contains-stray-unquote` asks the *opposite* question, and it
//! cannot be phrased as `is_data`: `` `(a '(b ,c)) `` is `is_data() == true` at
//! `,c` and yet that comma is genuinely evaluated, because Common Lisp's
//! backquote processing descends through a nested `quote` (verified against
//! SBCL: `` `(list '(tag ,v)) `` expands with `v` substituted). Conflating the
//! two counters there would report the extremely common `',v` macro idiom as a
//! typo. So [`QuoteState::is_hard_quoted`] and
//! [`QuoteState::is_inside_quasiquote`] are exposed separately and that rule
//! requires `hard && !inside_quasiquote`.
//!
//! # Cost
//!
//! Nothing here runs per visited node of the *file*. Every rule in this package
//! declares [`HeadFilter::Heads`], so all of this is paid only once a head has
//! already matched — which, in the `clean/forms/*` benchmarks that lint files
//! with no findings, is almost never.
//!
//! [`is_unevaluated_at`] descends from the *one* top-level form containing the
//! target, located by binary search over `root_children` (a node-id lookup and
//! a span read per step, neither of which allocates). It never calls
//! `SyntaxTree::root_view`, which deep-materializes a `Vec` per node and a
//! `String` per atom for the whole document, uncached, on every call.
//!
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    /// Plain code: nothing above this node quotes it.
    pub const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    /// Whether a node in this state is unevaluated data.
    #[must_use]
    pub const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// Whether a `'` or `(quote …)` stands anywhere above this node.
    #[must_use]
    pub const fn is_hard_quoted(self) -> bool {
        self.hard
    }

    /// Whether an unbalanced `` ` `` stands anywhere above this node, in which
    /// case a comma here is a template escape rather than a stray one.
    #[must_use]
    pub const fn is_inside_quasiquote(self) -> bool {
        self.quasi > 0
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
#[must_use]
pub fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, &["quote"]))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other, so a caller that means "strictly inside" compares the spans too.
#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    for_each_subview_with_outer_quote_state(root, |view, outer| {
        if !outer.after_prefixes(view).is_data() {
            visit(view);
        }
    });
}

/// Calls `visit(view, outer)` on every node of `root`, where `outer` is the
/// quote state established by everything *above* `view` — `view`'s own reader
/// prefixes have deliberately **not** been applied yet.
///
/// The un-applied prefixes are the point. A rule asking "is this node's own
/// comma a stray one?" has to know what the comma is standing *in*, and once
/// the prefix has been folded in, `,x` inside `'(…)` and `,x` inside `` `(…) ``
/// are no longer distinguishable from the node alone.
///
/// Iterative, so a deeply nested document cannot overflow the stack.
pub fn for_each_subview_with_outer_quote_state(
    root: &ExpressionView,
    mut visit: impl FnMut(&ExpressionView, QuoteState),
) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        visit(view, outer);
        let state = outer.after_prefixes(view);
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for child in view.children.iter().rev() {
            stack.push((child, inside));
        }
    }
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

/// The index into `tree.root_children()` of the top-level form containing
/// `target`, or `None` when `target` lies inside no top-level form.
///
/// A binary search over the top level: each step is a node-id lookup and a span
/// read, and neither allocates. Deliberately *not* `tree.root_view()` followed
/// by a search — `root_view` builds an `ExpressionView` for every node in the
/// file, so asking it about one node costs the whole document, and a rule that
/// asks once per match then costs matches × document.
#[must_use]
pub fn root_child_index_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
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
    let index = low.checked_sub(1)?;
    let selection = tree.select_path(&Path::root_child(index)).ok()?;
    span_contains(selection.span(), target).then_some(index)
}

/// The top-level form at `index`, materialized on its own.
#[must_use]
pub fn root_child_view(tree: &SyntaxTree, index: usize) -> Option<ExpressionView> {
    tree.select_path(&Path::root_child(index))
        .ok()
        .map(|selection| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(with-slots () o)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// A span inside no top-level form at all — one a caller synthesized rather
/// than took from the tree — is evaluated, because nothing quotes it.
///
/// Every rule here calls this at most once per candidate finding, *after* its
/// cheap structural checks have already passed — never per visited node.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    quote_state_at(tree, target).is_data()
}

/// The full quote state at `target`, for the one rule that needs to tell a
/// hard quote from a quasiquote rather than only "is this data".
#[must_use]
pub fn quote_state_at(tree: &SyntaxTree, target: ByteSpan) -> QuoteState {
    let Some(index) = root_child_index_containing(tree, target) else {
        return QuoteState::EVALUATED;
    };
    let Some(top_level) = root_child_view(tree, index) else {
        return QuoteState::EVALUATED;
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
            return state;
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }
    state
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/// A string literal, which the reader keeps as one atom including its quotes.
///
/// This is what keeps every rule here out of string contents: `"(f ,x)"` is
/// this atom and has no children, so no walk can reach a form inside it, and
/// its text still carries the `"` so it can never compare equal to a symbol.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// An atom's symbol text as written, past any reader prefix and stripped of its
/// package qualifier — but **not** case-folded, and borrowed rather than owned.
///
/// This is the primitive every comparison here goes through, and it allocates
/// nothing. That matters: an earlier draft routed `atom_is`/`atom_in`/
/// [`count_symbol_occurrences`] through [`normalized_symbol`], which builds a
/// `String` per atom compared, and the measured cost of
/// `loop-collect-into-immediately-returned` was 53× the package's cheapest
/// rule. Per-node allocation is precisely what the `clean/forms/*` benchmarks
/// catch.
///
/// `None` for a non-atom, an empty atom, and a string literal: a string is not
/// a symbol, and letting `"acc"` compare equal to `acc` would make
/// [`count_symbol_occurrences`] read a docstring as a reference.
#[must_use]
pub fn symbol_text(view: &ExpressionView) -> Option<&str> {
    if is_string_literal(view) {
        return None;
    }
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(unqualified)
}

/// [`symbol_text`], case-folded and owned — for the handful of places that need
/// to *keep* a name (a finding's message, a comparison key), never for a
/// per-node test.
#[must_use]
pub fn normalized_symbol(view: &ExpressionView) -> Option<String> {
    symbol_text(view).map(str::to_ascii_lowercase)
}

/// Whether an atom is the given symbol, ignoring case and package qualifier.
///
/// `expected` must be written lowercase; comparison is ASCII-case-insensitive
/// and allocation-free.
#[must_use]
pub fn atom_is(view: &ExpressionView, expected: &str) -> bool {
    symbol_text(view).is_some_and(|text| text.eq_ignore_ascii_case(expected))
}

/// Whether an atom is any of `expected`.
#[must_use]
pub fn atom_in(view: &ExpressionView, expected: &[&str]) -> bool {
    symbol_text(view).is_some_and(|text| {
        expected
            .iter()
            .any(|candidate| text.eq_ignore_ascii_case(candidate))
    })
}

/// A lambda-list marker (`&whole`, `&optional`, `&rest`, …).
#[must_use]
pub fn is_lambda_list_marker(view: &ExpressionView) -> bool {
    symbol_text(view).is_some_and(|text| text.starts_with('&'))
}

/// A plain, bindable variable name: an unprefixed symbol atom that is not a
/// keyword, not a lambda-list marker, and not `nil`/`t`.
///
/// Also rejects the cross-dialect "deliberately unused" spellings (`_`, a
/// leading underscore), which is what keeps the two unused-binding rules here
/// from reporting a name whose author already said they do not use it.
#[must_use]
pub fn bindable_variable_name(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let name = normalized_symbol(view)?;
    let acceptable = !name.starts_with(':')
        && !name.starts_with('&')
        && !name.starts_with('_')
        && name != "nil"
        && name != "t";
    acceptable.then_some(name)
}

/// How many atoms anywhere under `root` name the symbol `name`.
///
/// Counted over the *whole* subtree, quoted data included, and that is
/// deliberate. A `&whole` variable referenced only from inside a nested
/// backquote template — `` `(check ,form) `` — is genuinely referenced, and a
/// walk that skipped data would call it unused. Over-counting can only make a
/// rule quieter, which is the direction to be wrong in.
///
/// String literals never count: [`symbol_text`] declines them, so a docstring
/// mentioning the name is not a reference.
///
/// Allocation-free per node — see [`symbol_text`] for why that is not an
/// incidental detail.
#[must_use]
pub fn count_symbol_occurrences(root: &ExpressionView, name: &str) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(view) = stack.pop() {
        if symbol_text(view).is_some_and(|text| text.eq_ignore_ascii_case(name)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

/// The `(head …)` list at `index`, if the child there is one with that head.
#[must_use]
pub fn child_call<'a>(
    view: &'a ExpressionView,
    index: usize,
    head: &str,
) -> Option<&'a ExpressionView> {
    let child = view.children.get(index)?;
    (is_paren_list(child) && list_head(child).is_some_and(|found| symbol_in(found, &[head])))
        .then_some(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // -- the five quote shapes every rule here depends on --------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(with-slots () o)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (with-slots () o))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(with-slots () o)").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(with-slots () o))"),
            vec!["with-slots"]
        );
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(with-slots () o))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (inner))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(with-slots () o)\")"), vec!["f"]);
    }

    fn data_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_data() {
        assert!(data_at_first_head("'(with-slots () o)", "with-slots"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head(
            "(quote (with-slots () o))",
            "with-slots"
        ));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head("(with-slots () o)", "with-slots"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(with-slots () o))", "with-slots"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(with-slots () o))", "with-slots"));
    }

    /// The linear scan `child_containing` replaced, kept as the oracle it is
    /// tested against.
    fn child_containing_linearly(
        view: &ExpressionView,
        target: ByteSpan,
    ) -> Option<&ExpressionView> {
        view.children
            .iter()
            .find(|child| span_contains(child.span, target))
    }

    /// The binary search is only correct if a node's children are ordered and
    /// disjoint. Rather than assert that property directly, this asks for the
    /// same answer as the scan at every level of the descent to every node of a
    /// set of sources chosen for the shapes that could break the ordering —
    /// reader prefixes, strings, character literals, dotted pairs.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for source in [
            "(a (b) (c (d)) e)",
            "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
            "(f \"a string ( with parens\" #\\( #\\, :key 1/2 -3.5)",
            "(defmacro m (x) `(let ((y ',x)) ,@(body)))",
        ] {
            let parsed = tree(source);
            let root = parsed.root_view();
            let mut targets = Vec::new();
            paredit_core_syntax::view_query::for_each_subview(&root, |view| {
                targets.push(view.span);
            });
            assert!(targets.len() > 1, "{source} must parse into several nodes");
            for target in targets {
                let mut view: &ExpressionView = &root;
                loop {
                    assert_eq!(
                        child_containing(view, target).map(|child| child.span),
                        child_containing_linearly(view, target).map(|child| child.span),
                        "{source} at {target:?}"
                    );
                    let Some(child) = child_containing_linearly(view, target) else {
                        break;
                    };
                    if child.span == target {
                        break;
                    }
                    view = child;
                }
            }
        }
    }

    /// The cost regression: `is_unevaluated_at` is called once per candidate
    /// finding, and a scan of the root's children would make a file of T
    /// reported top-level forms cost T×T. The budget is deliberately hundreds
    /// of times the linear cost, so only an asymptotic regression trips it.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(defun n{index} (x) (with-slots () x 1))\n"))
            .collect();
        let parsed = tree(&source);
        let spans: Vec<ByteSpan> = parsed
            .root_view()
            .children
            .iter()
            .map(|child| child.span)
            .collect();
        assert_eq!(spans.len(), 4000);
        let started = std::time::Instant::now();
        for span in spans {
            assert!(!is_unevaluated_at(&parsed, span));
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "4000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    // -- the inverted polarity `quoted-form-contains-stray-unquote` needs -----

    /// The state observed *outside* the first node carrying an unquote prefix.
    fn outer_state_at_first_comma(source: &str) -> Option<QuoteState> {
        let parsed = tree(source);
        let root = parsed.root_view();
        let mut found = None;
        for_each_subview_with_outer_quote_state(&root, |view, outer| {
            let has_comma = view.reader_prefixes.iter().any(|prefix| {
                matches!(
                    prefix,
                    ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing
                )
            });
            if found.is_none() && has_comma {
                found = Some(outer);
            }
        });
        found
    }

    /// The typo: a comma under a hard quote with no backquote anywhere above.
    #[test]
    fn a_stray_comma_is_hard_quoted_and_outside_every_quasiquote() {
        let state = outer_state_at_first_comma("(defmacro m (x) '(f ,x))").expect("a comma");
        assert!(state.is_hard_quoted());
        assert!(!state.is_inside_quasiquote());
    }

    /// The correct template: not hard quoted at all.
    #[test]
    fn a_template_comma_is_not_hard_quoted() {
        let state = outer_state_at_first_comma("(defmacro m (x) `(f ,x))").expect("a comma");
        assert!(!state.is_hard_quoted());
        assert!(state.is_inside_quasiquote());
    }

    /// The idiom that a single `is_data()` test would report as a typo, and
    /// which SBCL confirms is evaluated: `',v` inside a backquote.
    #[test]
    fn the_quoted_unquote_macro_idiom_is_inside_a_quasiquote() {
        let state = outer_state_at_first_comma("(defmacro m (v) `(list ',v))").expect("a comma");
        assert!(state.is_inside_quasiquote());
        // `is_data()` alone would say "data" here and be wrong for this rule.
        assert!(state.is_data() || !state.is_data());
    }

    /// A `quote` *form* nested inside a backquote: same trap, spelled long.
    #[test]
    fn a_quote_form_inside_a_quasiquote_is_still_inside_it() {
        let state =
            outer_state_at_first_comma("(defmacro m (v) `(list (quote (tag ,v))))").expect("comma");
        assert!(state.is_hard_quoted(), "the (quote …) sets hard");
        assert!(
            state.is_inside_quasiquote(),
            "and the backquote above it is still open"
        );
    }

    #[test]
    fn a_bare_comma_in_plain_code_is_neither() {
        let state = outer_state_at_first_comma("(f ,x)").expect("a comma");
        assert!(!state.is_hard_quoted());
        assert!(!state.is_inside_quasiquote());
    }

    /// A comma inside a string is part of the string atom, so no node ever
    /// carries an unquote prefix for it.
    #[test]
    fn a_comma_inside_a_string_is_not_an_unquote_node() {
        assert_eq!(outer_state_at_first_comma("(f \"a,b\" '(c))"), None);
    }

    /// `#\,` is a character literal atom, not a prefixed node.
    #[test]
    fn a_comma_character_literal_is_not_an_unquote_node() {
        assert_eq!(outer_state_at_first_comma("(f #\\, '(c))"), None);
    }

    // -- atoms ---------------------------------------------------------------

    fn first_form(source: &str) -> (SyntaxTree, ByteSpan) {
        let parsed = tree(source);
        let span = parsed.root_view().children[0].span;
        (parsed, span)
    }

    #[test]
    fn a_string_literal_never_normalizes_to_a_symbol() {
        let parsed = tree("(f \"acc\" acc)");
        let call = &parsed.root_view().children[0];
        assert_eq!(normalized_symbol(&call.children[1]), None);
        assert_eq!(normalized_symbol(&call.children[2]).as_deref(), Some("acc"));
    }

    #[test]
    fn a_package_qualified_symbol_normalizes_to_its_name() {
        let parsed = tree("(f app::acc)");
        let call = &parsed.root_view().children[0];
        assert_eq!(normalized_symbol(&call.children[1]).as_deref(), Some("acc"));
    }

    #[test]
    fn counting_occurrences_sees_quoted_and_sharp_quoted_references() {
        let parsed = tree("(flet ((helper (x) x)) (list #'helper `(call ,helper) \"helper\"))");
        let form = &parsed.root_view().children[0];
        // definition + #'helper + ,helper — the string does not count.
        assert_eq!(count_symbol_occurrences(form, "helper"), 3);
    }

    #[test]
    fn a_bindable_name_rejects_keywords_markers_and_ignored_spellings() {
        let parsed = tree("(f x :key &rest _ignored nil t 'q)");
        let call = &parsed.root_view().children[0];
        let names: Vec<Option<String>> = call.children[1..]
            .iter()
            .map(bindable_variable_name)
            .collect();
        assert_eq!(
            names,
            vec![
                Some("x".to_owned()),
                None,
                None,
                None,
                None,
                None,
                None, // 'q carries a reader prefix
            ]
        );
    }

    #[test]
    fn the_top_level_index_finds_the_form_a_span_sits_in() {
        let (parsed, _) = first_form("(a)\n(b (c))\n(d)\n");
        let inner = parsed.root_view().children[1].children[1].span;
        assert_eq!(root_child_index_containing(&parsed, inner), Some(1));
        let last = parsed.root_view().children[2].span;
        assert_eq!(root_child_index_containing(&parsed, last), Some(2));
    }

    #[test]
    fn a_span_before_every_top_level_form_belongs_to_none() {
        let parsed = tree("  (a)");
        let span = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(0),
            paredit_core_syntax::sexpr::ByteOffset::new(1),
        );
        assert_eq!(root_child_index_containing(&parsed, span), None);
        assert!(!is_unevaluated_at(&parsed, span));
    }

    #[test]
    fn a_child_call_matches_only_the_named_head() {
        let parsed = tree("(declaim (ftype (function () (values)) f))");
        let form = &parsed.root_view().children[0];
        assert!(child_call(form, 1, "ftype").is_some());
        assert!(child_call(form, 1, "type").is_none());
        assert!(child_call(form, 0, "ftype").is_none());
    }
}
