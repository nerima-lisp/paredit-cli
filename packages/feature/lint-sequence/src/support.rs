//! What the newer rules in this package share: which parts of a file are
//! *code*, which list literal is a constant, and which form encloses a given
//! one.
//!
//! # Quote semantics
//!
//! `QuoteState`, `is_quote_form` and [`for_each_evaluated_subview`] are
//! copied from `paredit-feature-lint-condition-system`'s `support.rs`, tests
//! included, and [`is_unevaluated_at`] is copied from
//! `paredit-feature-lint-testing`'s — deliberately as a copy rather than as a
//! dependency: a lint feature package depending on another lint feature package
//! would be a new feature→feature edge for a hundred lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A node one level *inside*
//! a quote carries no reader prefix of its own and is still data, which is why
//! the state is threaded down the descent rather than read off the node.
//!
//! Clojure spells unquote `~`/`~@` and treats `,` as whitespace; the reader
//! maps both spellings onto the same [`ReaderPrefix::Unquote`] and
//! [`ReaderPrefix::UnquoteSplicing`], so the same three functions serve both
//! dialects. The tests pin the Clojure spelling separately, because a test
//! written with `,` in a Clojure source proves nothing at all.
//!
//! # Cost
//!
//! Nothing here runs per visited node.
//!
//! Every rule that calls [`is_unevaluated_at`] or [`enclosing_form`] does so
//! only *after* its own head has matched and its own cheap structural
//! predicates have already accepted the node — that is, once per candidate
//! finding, never once per node. Both descend from the enclosing top-level
//! form, located by a binary search over `root_children` that reads one span
//! per step and allocates nothing, so their cost is the size of that one form
//! and never the size of the file. Neither ever calls
//! [`SyntaxTree::root_view`], which materializes a `Vec` of children and a
//! `String` per atom for *every node in the document*, uncached, on every call.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};

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

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
///
/// Not called by any rule's `check`: this is what the corpus sweep counts
/// candidates with, so that the sweep agrees with the rules about what is code.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
        }
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

/// The index of the one child of `view` whose span covers `target`, found
/// without reading the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_index_containing(view: &ExpressionView, target: ByteSpan) -> Option<usize> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let index = after.checked_sub(1)?;
    span_contains(view.children.get(index)?.span, target).then_some(index)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document, uncached, on every call.
///
/// Selecting the one root child instead costs a binary search over the top
/// level — each step a node-id lookup and a span read, neither of which
/// allocates — plus that one form's own subtree.
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
/// data does not settle it: `` `(a ,(nth 0 x)) `` has a quasiquoted ancestor and
/// an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    quote_state_at(tree, target).is_data()
}

/// Whether the node at `target` sits inside a hard quote — `'(…)` or
/// `(quote …)` — and is therefore literal data in every expansion.
///
/// The guard a *rewriting* rule wants, where [`is_unevaluated_at`] is the guard
/// a *reporting* rule wants. The difference is the quasiquote, and it is not a
/// stylistic preference: `` `(nthcdr 0 ,x) `` is a template whose `nthcdr` really
/// is emitted as code, so a rule that suppressed itself there would go quiet on
/// exactly the macro bodies it exists to read. `'(nthcdr 0 x)` is a
/// three-element list of symbols, and rewriting it to `'x` edits a user's *data
/// literal* — which is why the verdict is read on the `hard` counter alone, and
/// never on `QuoteState::is_data`.
///
/// The target's *own* reader prefixes count, and every rule guarded by this
/// passes the span of the form it matched. That is deliberate: whether such a
/// rule's fix replaces `view.span` (prefix included) or `view.content_span`
/// (past it), rewriting a `'`-carrying form is either an edit to a datum or a
/// deletion of the `'` itself, and both are corruption. A rule whose *premise*
/// is a quote — one that exists to remove the very `'` it reads — would need
/// the enclosing state instead, and this package has none: no rule here matches
/// a `quote` head.
///
/// Costs one binary search over the top level plus one descent through the
/// enclosing top-level form — never [`SyntaxTree::root_view`] — and is meant to
/// be called only once a rule already holds a finding, so a file with no
/// findings never pays for it.
#[must_use]
pub fn is_hard_quoted_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    quote_state_at(tree, target).hard
}

/// The quote state at `target`: the one descent both [`is_unevaluated_at`] and
/// [`is_hard_quoted_at`] read their verdict off.
///
/// One implementation rather than two, because the failure mode of two is that
/// they drift: the `hard`-only reading and the `is_data` reading have to agree
/// about `` `(a ,x) `` or the guard and the report disagree about the same node.
fn quote_state_at(tree: &SyntaxTree, target: ByteSpan) -> QuoteState {
    let Some(top_level) = root_child_containing(tree, target) else {
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
        let Some(index) = child_index_containing(view, target) else {
            return state;
        };
        let child = &view.children[index];
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }
    state
}

/// The innermost form that strictly contains `target`, or `None` when `target`
/// is a top-level form (or names no node at all).
///
/// The one thing `HeadFilter::Heads` cannot answer on its own: a rule sees the
/// node it matched and nothing above it, so "is this call itself the argument
/// of another call of the same operator?" needs a descent from the root. Same
/// binary search, same cost, and called for the same reason — only once a
/// candidate finding already exists.
#[must_use]
pub fn enclosing_form(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let mut current = root_child_containing(tree, target)?;
    if current.span == target {
        return None;
    }
    loop {
        let Some(index) = child_index_containing(&current, target) else {
            return Some(current);
        };
        if current.children[index].span == target {
            return Some(current);
        }
        // Descends by moving the child out; `current` is discarded either way,
        // so reordering its remaining children is harmless and this clones
        // nothing.
        current = current.children.swap_remove(index);
    }
}

/// The elements of a hard-quoted list literal, or `None` for anything else.
///
/// # The inverted polarity
///
/// Every other use of this module's quote machinery asks "is this node data?"
/// in order to *skip* it. This one is the opposite: `(member x '(a b c))` has
/// its list quoted because that is the correct and expected way to write it, so
/// the rule that reads such a list has to descend into data on purpose. The two
/// are not in tension — the rule still asks [`is_unevaluated_at`] about the
/// `member` *call*, so a `member` inside `'(…)` is not a call at all, and only
/// then reads its already-quoted argument as the literal it is.
///
/// Only a *hard* quote counts, and only one:
///
/// - `` `(a b ,x) `` is not a constant — `,x` is evaluated — so a quasiquote is
///   never accepted, whether or not it happens to contain an unquote.
/// - `''(a b c)` is the two-element list `(quote (a b c))`, not `(a b c)`, so a
///   second `'` disqualifies it rather than being ignored.
/// - `(quote (a b c))` is the long hand for the same thing and is accepted, as
///   long as the inner list carries no reader prefix of its own.
#[must_use]
pub fn hard_quoted_list_children(view: &ExpressionView) -> Option<&[ExpressionView]> {
    if is_paren_list(view) && view.reader_prefixes == [ReaderPrefix::Quote] {
        return Some(&view.children);
    }
    if !is_quote_form(view) || !view.reader_prefixes.is_empty() || view.children.len() != 2 {
        return None;
    }
    let quoted = &view.children[1];
    (is_paren_list(quoted) && quoted.reader_prefixes.is_empty()).then_some(&*quoted.children)
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/// A reader-conditional atom (`#+feature`/`#-feature`), which the dialect-aware
/// parse folds into a single opaque atom. A form containing one has no settled
/// operand list, so no rule here reads one.
#[must_use]
pub fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// A plain symbol atom: something with atom text, no children, and not a
/// string, a number, a character, or a reader conditional.
///
/// Deliberately conservative — anything it cannot positively identify as a
/// symbol is not one, so a rule that counts symbols under-counts rather than
/// mistakes a literal for a name.
#[must_use]
pub fn is_symbol_atom(view: &ExpressionView) -> bool {
    let Some(text) = atom_text(view) else {
        return false;
    };
    if !view.children.is_empty() || text.is_empty() || is_reader_conditional(view) {
        return false;
    }
    let first = text.as_bytes()[0];
    // A string literal arrives as one atom including its quotes; `#\a` is a
    // character; a leading digit, sign or dot starts a number token.
    !matches!(first, b'"' | b'#') && !first.is_ascii_digit() && !matches!(first, b'-' | b'+' | b'.')
}

/// Runs one rule end to end through the real lint engine and returns, per
/// finding, its message and the source that applying its fix produces.
///
/// A domain test cannot see the thing that actually broke these rules on real
/// code, because the domain never applies a fix: a replacement region that
/// starts one byte early deletes the form's reader prefix, and every assertion
/// about spans and counts still passes. Only the spliced source shows it. So
/// this returns the rewritten text, not just the finding.
#[cfg(test)]
#[must_use]
pub fn run_rule_fixed(
    entries: &'static [paredit_core_lint_engine::rule::RuleEntry],
    source: &str,
) -> Vec<(String, String)> {
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::RuleCatalog;
    use paredit_core_syntax::dialect::Dialect;

    let catalog = RuleCatalog::new(entries);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    collect_lint_outcomes(
        catalog,
        &index,
        std::path::Path::new("app.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| {
        let (finding, fix) = outcome.into_parts();
        let mut fixed = source.to_owned();
        if let Some(fix) = fix {
            // Highest offset first, so an earlier edit's span stays valid.
            let mut edits: Vec<_> = fix.replacements().collect();
            edits.sort_by_key(|edit| std::cmp::Reverse(edit.span().start().get()));
            for edit in edits {
                fixed.replace_range(
                    edit.span().start().get()..edit.span().end().get(),
                    edit.text(),
                );
            }
        }
        (finding.message, fixed)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str, dialect: Dialect) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, dialect).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        evaluated_heads_in(source, Dialect::CommonLisp)
    }

    fn evaluated_heads_in(source: &str, dialect: Dialect) -> Vec<String> {
        let parsed = tree(source, dialect);
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
        assert!(evaluated_heads("'(nth 0 x)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (nth 0 x))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(nth 0 x)").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(evaluated_heads("`(a ,(nth 0 (f x)))"), vec!["nth", "f"]);
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(nth 0 x))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (inner))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(nth 0 x)\")"), vec!["f"]);
    }

    // -- the same five shapes in Clojure, where unquote is `~` ---------------

    /// `,` is *whitespace* in Clojure, so a test written with a comma would
    /// pass for the wrong reason. These pin the `~`/`~@` spellings.
    #[test]
    fn clojure_syntax_quote_and_unquote_use_the_tilde_spelling() {
        assert!(evaluated_heads_in("'(get m :a)", Dialect::Clojure).is_empty());
        assert!(evaluated_heads_in("`(get m :a)", Dialect::Clojure).is_empty());
        assert_eq!(
            evaluated_heads_in("`(a ~(get m :a))", Dialect::Clojure),
            vec!["get"]
        );
        assert_eq!(
            evaluated_heads_in("`(a ~@(get m :a))", Dialect::Clojure),
            vec!["get"]
        );
        // A comma inside a hard quote is not an escape in Common Lisp, and in
        // Clojure it is not even punctuation — either way this stays data.
        assert!(evaluated_heads_in("'(a ,(get m :a))", Dialect::Clojure).is_empty());
    }

    fn data_at_first_head(source: &str, head: &str) -> bool {
        data_at_first_head_in(source, head, Dialect::CommonLisp)
    }

    fn data_at_first_head_in(source: &str, head: &str, dialect: Dialect) -> bool {
        let parsed = tree(source, dialect);
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
        assert!(data_at_first_head("'(nth 0 x)", "nth"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head("(quote (nth 0 x))", "nth"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head("(nth 0 x)", "nth"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(nth 0 x))", "nth"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(nth 0 x))", "nth"));
    }

    #[test]
    fn a_clojure_span_under_a_tilde_reads_as_evaluated() {
        assert!(!data_at_first_head_in(
            "`(a ~(get m :a))",
            "get",
            Dialect::Clojure
        ));
        assert!(data_at_first_head_in(
            "`(a (get m :a))",
            "get",
            Dialect::Clojure
        ));
    }

    /// A top-level form is judged by its own prefixes and nothing shallower.
    #[test]
    fn a_top_level_form_is_evaluated_and_a_quoted_one_is_not() {
        assert!(!data_at_first_head("(f x)\n(nth 0 x)\n", "nth"));
        assert!(data_at_first_head("(f x)\n'(nth 0 x)\n", "nth"));
    }

    // -- enclosing_form ------------------------------------------------------

    fn span_of_nth_head(parsed: &SyntaxTree, head: &str, skip: usize) -> ByteSpan {
        let mut spans = Vec::new();
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if list_head(view).is_some_and(|found| found == head) {
                spans.push(view.span);
            }
        });
        spans[skip]
    }

    #[test]
    fn a_top_level_form_has_no_enclosing_form() {
        let parsed = tree("(get m :a)", Dialect::Clojure);
        let span = span_of_nth_head(&parsed, "get", 0);
        assert!(enclosing_form(&parsed, span).is_none());
    }

    #[test]
    fn an_inner_form_is_enclosed_by_the_form_it_is_an_argument_of() {
        let parsed = tree("(get (get m :a) :b)", Dialect::Clojure);
        // The second `get` in document order is the inner one.
        let inner = span_of_nth_head(&parsed, "get", 1);
        let parent = enclosing_form(&parsed, inner).expect("an enclosing form");
        assert_eq!(list_head(&parent), Some("get"));
        assert_eq!(parent.children[1].span, inner);
    }

    #[test]
    fn the_outermost_form_of_a_chain_is_enclosed_by_something_that_is_not_a_get() {
        let parsed = tree("(println (get (get m :a) :b))", Dialect::Clojure);
        let outer = span_of_nth_head(&parsed, "get", 0);
        let parent = enclosing_form(&parsed, outer).expect("an enclosing form");
        assert_eq!(list_head(&parent), Some("println"));
    }

    /// The binary-search descent must agree with a plain linear scan; the
    /// shapes chosen are the ones that could break the "ordered and disjoint"
    /// assumption it rests on.
    #[test]
    fn the_enclosing_form_is_what_a_linear_scan_would_find() {
        for (source, dialect) in [
            ("(a (b) (c (d)) e)", Dialect::CommonLisp),
            (
                "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
                Dialect::CommonLisp,
            ),
            (
                "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
                Dialect::CommonLisp,
            ),
            (
                "(get ^:m (get m :a) :b) #{1 2} [1 2] {:a 1}",
                Dialect::Clojure,
            ),
        ] {
            let parsed = tree(source, dialect);
            let root = parsed.root_view();
            // Per top-level form, because the root is not itself a form: a file
            // with one top-level form has a root whose span *equals* that
            // form's, so a root-based oracle cannot express "has no enclosing
            // form" at all.
            for top in &root.children {
                paredit_core_syntax::view_query::for_each_subview(top, |view| {
                    let expected = linear_parent(top, view.span).map(|parent| parent.span);
                    let found = enclosing_form(&parsed, view.span).map(|parent| parent.span);
                    assert_eq!(found, expected, "{source} at {:?}", view.span);
                });
            }
        }
    }

    /// The scan `enclosing_form`'s descent replaced, kept as its oracle.
    fn linear_parent(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
        if view.span == target {
            return None;
        }
        let child = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))?;
        linear_parent(child, target).or(Some(view))
    }

    // -- atoms ---------------------------------------------------------------

    #[test]
    fn a_symbol_atom_is_one_and_a_literal_is_not() {
        let parsed = tree(
            "(f alpha \"s\" 12 -3 #\\a #+sbcl x :key)",
            Dialect::CommonLisp,
        );
        let root = parsed.root_view();
        let call = &root.children[0];
        let kinds: Vec<bool> = call.children.iter().map(is_symbol_atom).collect();
        assert_eq!(
            kinds,
            vec![true, true, false, false, false, false, false, true]
        );
    }

    // -- hard_quoted_list_children -------------------------------------------

    /// The argument of the first `member` in `source`, read as a quoted list.
    fn quoted_argument(source: &str, dialect: Dialect) -> Option<Vec<String>> {
        let parsed = tree(source, dialect);
        let root = parsed.root_view();
        let call = &root.children[0];
        hard_quoted_list_children(&call.children[2]).map(|children| {
            children
                .iter()
                .map(|child| atom_text(child).unwrap_or("<list>").to_owned())
                .collect()
        })
    }

    fn cl_quoted_argument(source: &str) -> Option<Vec<String>> {
        quoted_argument(source, Dialect::CommonLisp)
    }

    #[test]
    fn a_hard_quoted_list_yields_its_elements() {
        assert_eq!(
            cl_quoted_argument("(member x '(a b c))"),
            Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
    }

    #[test]
    fn the_long_hand_quote_form_yields_the_same_elements() {
        assert_eq!(
            cl_quoted_argument("(member x (quote (a b c)))"),
            Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
    }

    /// `` `(a b ,x) `` is not a constant, so it is never read as one — with or
    /// without an unquote actually in it.
    #[test]
    fn a_quasiquoted_list_is_not_a_constant() {
        assert_eq!(cl_quoted_argument("(member x `(a b c))"), None);
        assert_eq!(cl_quoted_argument("(member x `(a b ,c))"), None);
    }

    /// `''(a b c)` is the two-element list `(quote (a b c))`, not `(a b c)`.
    #[test]
    fn a_doubly_quoted_list_is_not_the_list_it_looks_like() {
        assert_eq!(cl_quoted_argument("(member x ''(a b c))"), None);
    }

    #[test]
    fn an_unquoted_list_and_a_variable_are_not_quoted_literals() {
        assert_eq!(cl_quoted_argument("(member x (list a b c))"), None);
        assert_eq!(cl_quoted_argument("(member x known)"), None);
        assert_eq!(cl_quoted_argument("(member x '())"), Some(Vec::new()));
    }

    #[test]
    fn a_list_is_not_a_symbol_atom() {
        let parsed = tree("(f (g x))", Dialect::CommonLisp);
        let root = parsed.root_view();
        assert!(!is_symbol_atom(&root.children[0].children[1]));
    }
}
