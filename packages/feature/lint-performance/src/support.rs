//! What the three correlating cost rules share: which parts of a form are
//! *code*, and which parts of a body are mutually exclusive branches.
//!
//! [`crate::shared`] answers "does this expression change from round to round?",
//! which is the question the loop rules ask. The rules added later ask two
//! different ones, and neither the engine nor `core/syntax` answers either:
//!
//! - **Evaluation context.** `'(mapcar #'f (mapcar #'g xs))` is a list of
//!   symbols, not two calls. The engine's dispatch walks into quoted data like
//!   any other subtree and [`RuleContext`] carries no parent pointer, so a
//!   head-matched node cannot tell on its own whether it is code.
//!   [`is_unevaluated_at`] answers that by descending from the *enclosing
//!   top-level form* — never from `tree.root_view()`, see its own docs — and is
//!   called only once a rule already has a finding to report.
//!
//! - **Branch exclusivity.** Two `gethash` lookups in the two arms of an `if`
//!   are not a repeated lookup: only one of them runs.
//!   [`for_each_evaluated_subview_with_branches`] tags each visited node with
//!   the conditional arms it sits under, and [`branches_are_exclusive`] reads
//!   two such tags. Without it, `repeated-hash-table-lookup-same-key` would
//!   report the single most ordinary shape in the language.
//!
//! # Quote semantics
//!
//! [`QuoteState`] is copied from `paredit-feature-lint-condition-system`'s
//! `support.rs`, tests included, deliberately as a copy rather than as a
//! dependency: a lint feature package depending on another lint feature package
//! would be a new feature→feature edge for a hundred lines of traversal, and
//! `paredit-feature-lint-testing` made the same copy for the same reason.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A node one level *inside*
//! a quote is still data, so a node-local `reader_prefixes` check is not enough
//! either. Both shapes are pinned by tests below.
//!
//! # Cost
//!
//! Nothing here runs per visited node of the *document*. Each caller is
//! `HeadFilter::Heads`-anchored, so the walk below runs only over an already
//! matched subtree, and [`is_unevaluated_at`] runs only once a candidate finding
//! exists. The `clean/forms/*` benchmarks lint files with no findings, so the
//! per-file cost of a rule that matches nothing is exactly what they measure —
//! and for two of the three callers that cost is zero.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_is, unqualified};

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data. `#'` matters here — every `mapcar` argument is
    /// spelled `#'f` — and reading it as a quote would make the fusable-maps
    /// rule silent on its own canonical example.
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

/// Whether `outer` covers every byte of `inner`.
const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The one child of `view` whose span covers `target`, found without reading the
/// others.
///
/// A node's children are in document order and do not overlap, so the only child
/// that can contain `target` is the last one beginning at or before it — which a
/// binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let child = view.children.get(after.checked_sub(1)?)?;
    span_contains(child.span, target).then_some(child)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `String` per atom —
/// for *every node in the file*, uncached, so asking it about one node costs the
/// whole document. `paredit-feature-lint-testing` measured that as 34s inside a
/// single rule on 4000 definitions; the technique here is the one that replaced
/// it.
///
/// Selecting the one root child instead costs a binary search over the top level
/// — each step a node-id lookup and a span read, neither of which allocates —
/// plus that one form's own subtree.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
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
/// it, so the cost is the enclosing top-level form's size, and never the file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(mapcar #'f (mapcar #'g xs))) `` has a
/// quasiquoted ancestor and an evaluated target. Being inside a hard `'` does
/// settle it, and that is already modelled by `hard` never clearing.
///
/// A span inside no top-level form at all — one a caller synthesized rather than
/// took from the tree — is evaluated, because nothing quotes it.
///
/// Every rule here calls this at most once per candidate finding, *after* its
/// head filter has already matched — never per visited node.
#[must_use]
pub(crate) fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
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

// ---------------------------------------------------------------------------
// Branch exclusivity
// ---------------------------------------------------------------------------

/// One conditional a node sits inside: `(the conditional's span start, which of
/// its arms)`.
///
/// The span start identifies the conditional node, which is enough because two
/// distinct nodes in one document cannot begin at the same byte.
pub(crate) type BranchStep = (usize, usize);

/// Where a form's mutually exclusive arms begin, for the forms that have any.
///
/// Only the children from the returned index onward are arms; everything before
/// it runs unconditionally relative to the form. `(if TEST then else)` puts the
/// test at index 1 and its two arms at 2 and 3, so a lookup in the test and a
/// lookup in the `then` arm are *not* exclusive — when the `then` arm runs, the
/// test ran too, and the second lookup really is a repeat.
///
/// `and`/`or` are listed because a later operand only runs when the earlier ones
/// short-circuited into it — which is a prefix relation, not an exclusion, and
/// is exactly what the model above expresses.
///
/// `loop` is listed for a different reason: extended `loop` spells its
/// `if`/`when`/`else` clauses as a flat run of symbols and forms among the
/// iteration clauses, which this module does not parse. Giving every child its
/// own arm index means no two nodes *inside* one `loop` are ever paired, which
/// is the conservative reading. A node outside the loop and a node inside it
/// still pair, because that is a prefix relation and the inner lookup genuinely
/// repeats the outer one.
///
/// `when`/`unless` are deliberately absent: their body is one arm, so a lookup
/// in the test and a lookup in the body are a prefix relation and the pairing is
/// correct.
/// The arm-bearing forms, paired with the child index their arms start at.
///
/// A table read with one [`unqualified`] call rather than thirteen. `symbol_in`
/// re-derives the head's unqualified spelling once per candidate name, which is
/// a string scan per comparison; on a walk that asks this of every list node
/// that is thirteen scans per node, and it measured as the dominant cost of the
/// one rule that walks a body. The comparison below strips the qualifier once
/// and then only compares.
const ARM_HEADS: [(&str, usize); 13] = [
    ("if", 2),
    ("or", 2),
    ("and", 2),
    ("case", 2),
    ("cond", 1),
    ("loop", 1),
    ("ccase", 2),
    ("ecase", 2),
    ("typecase", 2),
    ("ctypecase", 2),
    ("etypecase", 2),
    ("handler-case", 2),
    ("restart-case", 2),
];

fn branch_start(view: &ExpressionView) -> Option<usize> {
    let name = unqualified(list_head(view)?);
    ARM_HEADS
        .iter()
        .find(|(head, _)| head.eq_ignore_ascii_case(name))
        .map(|(_, start)| *start)
}

/// Whether two nodes' branch tags put them in arms that cannot both run.
///
/// Exclusive exactly when the two paths agree up to some position and then name
/// *different arms of the same conditional*. Naming different conditionals at
/// that position means the two nodes are in unrelated subtrees, both of which
/// can run — `(progn (when p (if a X)) (when q (if b Y)))` is not an exclusion.
/// One path being a prefix of the other is not an exclusion either: whenever the
/// deeper node runs, the shallower one already ran.
#[must_use]
pub(crate) fn branches_are_exclusive(left: &[BranchStep], right: &[BranchStep]) -> bool {
    for (here, there) in left.iter().zip(right) {
        if here == there {
            continue;
        }
        return here.0 == there.0;
    }
    false
}

/// Calls `visit` on every node of `root` reachable as evaluated code, together
/// with the conditional arms it sits under.
///
/// `visit` answers whether to descend into the node's children; answering
/// `false` visits that node and nothing under it, which is how a caller skips a
/// `(declare …)` without a second closure.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited. A pruned node's subtree is skipped
/// including any quoted data in it, which is correct here and would not be if
/// this were used to find data.
///
/// The branch path is one `Vec` reused across the whole walk, pushed and popped
/// at conditionals only, so a node visit allocates nothing.
pub(crate) fn for_each_evaluated_subview_with_branches(
    root: &ExpressionView,
    mut visit: impl FnMut(&ExpressionView, &[BranchStep]) -> bool,
) {
    let mut path: Vec<BranchStep> = Vec::new();
    descend(root, QuoteState::EVALUATED, &mut path, &mut visit);
}

fn descend(
    view: &ExpressionView,
    outer: QuoteState,
    path: &mut Vec<BranchStep>,
    visit: &mut impl FnMut(&ExpressionView, &[BranchStep]) -> bool,
) {
    let state = outer.after_prefixes(view);
    if !state.is_data() && !visit(view, path) {
        return;
    }
    let inside = if is_quote_form(view) {
        state.quoted()
    } else {
        state
    };
    let arms = branch_start(view).map(|start| (start, view.span.start().get()));
    for (index, child) in view.children.iter().enumerate() {
        match arms {
            Some((start, marker)) if index >= start => {
                path.push((marker, index));
                descend(child, inside, path, visit);
                path.pop();
            }
            _ => descend(child, inside, path, visit),
        }
    }
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/// Running one rule through the *real* engine, which is the only way to observe
/// what the CLI would observe.
///
/// A rule's `examine`-style function is not the rule: the engine's dispatch
/// decides which nodes reach `check` at all, and it walks into quoted data. A
/// test that calls `examine` directly cannot see a rule that fires on `'(…)`,
/// which is precisely the false positive the quote handling above exists to
/// prevent.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path as FilePath;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::model::RuleMeta;
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// Every message one rule reports over `source`, through the engine's own
    /// dispatch.
    ///
    /// Leaks a one-entry catalogue because [`RuleCatalog`] is `'static` by
    /// design — the shipped one is a compile-time constant. A handful of leaked
    /// `RuleEntry` values in a test process is not a leak anyone can observe.
    pub(crate) fn messages(
        meta: &'static RuleMeta,
        rule: &'static (dyn LintRule + Sync),
        source: &str,
    ) -> Vec<String> {
        let entries: &'static [RuleEntry] = Box::leak(Box::new([RuleEntry::new(meta, rule)]));
        let catalog = RuleCatalog::new(entries);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            FilePath::new("probe.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.message)
        .collect()
    }

    /// The exact source text of every span one rule reports over `source`.
    pub(crate) fn reported(
        meta: &'static RuleMeta,
        rule: &'static (dyn LintRule + Sync),
        source: &str,
    ) -> Vec<String> {
        let entries: &'static [RuleEntry] = Box::leak(Box::new([RuleEntry::new(meta, rule)]));
        let catalog = RuleCatalog::new(entries);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            FilePath::new("probe.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint")
        .into_iter()
        .map(|outcome| {
            let span = outcome.into_parts().0.span;
            source[span.start().get()..span.end().get()].to_owned()
        })
        .collect()
    }
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
        for_each_evaluated_subview_with_branches(&parsed.root_view(), |view, _| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
            true
        });
        heads
    }

    // -- the five quote shapes every rule here depends on ---------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(mapcar (gethash :a h))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(
            evaluated_heads("(quote (mapcar (gethash :a h)))"),
            vec!["quote"]
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(mapcar (gethash :a h))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(mapcar (gethash :a h)))"),
            vec!["mapcar", "gethash"]
        );
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a hard
    /// quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(mapcar (gethash :a h)))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner node
    /// carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (inner))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(gethash :a h)\")"), vec!["f"]);
    }

    /// `#'f` is a function *reference*, not a quotation, and every `mapcar`
    /// argument is spelled that way. Reading it as a quote would make the
    /// fusable-maps rule silent on its own canonical example.
    ///
    /// The `"x"` in the expectation is the lambda list `(x)`, whose head is the
    /// parameter — the walk does not know a lambda list from a call, and does
    /// not need to.
    #[test]
    fn a_function_reference_prefix_does_not_make_its_form_data() {
        assert_eq!(
            evaluated_heads("(mapcar #'(lambda (x) (g x)) xs)"),
            vec!["mapcar", "lambda", "x", "g"]
        );
    }

    // -- span-directed lookup ------------------------------------------------

    fn data_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node even
        // when it is data.
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_data() {
        assert!(data_at_first_head("'(sort xs)", "sort"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head("(quote (sort xs))", "sort"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head("(defun f () (sort xs))", "sort"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(sort xs))", "sort"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(sort xs))", "sort"));
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

    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for source in [
            "(a (b) (c (d)) e)",
            "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
            "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
            "(defun f (h) (gethash :a h) (sort xs #'<))",
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

    /// The cost regression: resolving a span must cost the enclosing top-level
    /// form, not the file.
    ///
    /// The budget is an absolute one, not a ratio, and that is deliberate: this
    /// batch removed several wall-clock *ratio* assertions after one failed CI,
    /// because the ratio of two short durations has no safe threshold. An
    /// absolute bound does, when it is measured. On the equivalent fixture the
    /// descent takes ~21 ms in the `test` profile and the `root_view()` shape
    /// projects to ~34 s, so 10 s sits ~485× above the real cost and ~3.4×
    /// below the regression. Re-measure rather than adjust the constant if the
    /// fixture shrinks or these tests start running in `--release`.
    #[test]
    fn resolving_a_span_does_not_scan_the_whole_document() {
        let source: String = (0..4000)
            .map(|index| format!("(defun n{index} (h) (gethash :a h))\n"))
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
            "4000 lookups took {elapsed:?}; the descent is scanning the whole document again"
        );
    }

    // -- branch exclusivity --------------------------------------------------

    /// The branch tags of every `(mark …)` call in a source, in document order.
    fn marks(source: &str) -> Vec<Vec<BranchStep>> {
        let parsed = tree(source);
        let mut found = Vec::new();
        for_each_evaluated_subview_with_branches(&parsed.root_view(), |view, path| {
            if list_head(view).is_some_and(|head| head == "mark") {
                found.push(path.to_vec());
            }
            true
        });
        found
    }

    fn exclusive(source: &str) -> bool {
        let tags = marks(source);
        assert_eq!(tags.len(), 2, "{source} must contain exactly two marks");
        branches_are_exclusive(&tags[0], &tags[1])
    }

    #[test]
    fn the_two_arms_of_an_if_are_exclusive() {
        assert!(exclusive("(if p (mark 1) (mark 2))"));
    }

    #[test]
    fn an_ifs_test_and_its_arm_are_not_exclusive() {
        // When the arm runs, the test ran. The second read really is a repeat.
        assert!(!exclusive("(if (mark 1) (mark 2) nil)"));
    }

    #[test]
    fn two_cond_clauses_are_exclusive() {
        assert!(exclusive("(cond ((mark 1) :a) (t (mark 2)))"));
    }

    #[test]
    fn two_case_clauses_are_exclusive() {
        assert!(exclusive("(case k (:a (mark 1)) (:b (mark 2)))"));
    }

    #[test]
    fn a_cases_keyform_is_not_exclusive_with_its_clauses() {
        assert!(!exclusive("(case (mark 1) (:a (mark 2)))"));
    }

    #[test]
    fn two_forms_in_one_body_are_not_exclusive() {
        assert!(!exclusive("(progn (mark 1) (mark 2))"));
    }

    #[test]
    fn a_when_body_is_not_exclusive_with_its_test() {
        assert!(!exclusive("(when (mark 1) (mark 2))"));
    }

    #[test]
    fn two_ands_operands_are_a_prefix_relation_not_an_exclusion() {
        // The second only runs when the first returned true — at which point
        // both ran.
        assert!(!exclusive("(and (mark 1) (mark 2))"));
    }

    #[test]
    fn different_conditionals_at_the_same_depth_are_not_exclusive() {
        assert!(!exclusive(
            "(progn (when p (if a (mark 1) nil)) (when q (if b (mark 2) nil)))"
        ));
    }

    #[test]
    fn two_clauses_of_one_loop_are_never_paired() {
        // Extended `loop` spells `if`/`else` as a flat clause run this module
        // does not parse, so nothing inside one loop pairs with anything else
        // inside it.
        assert!(exclusive("(loop for x in xs if (mark 1) do (mark 2))"));
    }

    #[test]
    fn a_form_outside_a_loop_still_pairs_with_one_inside_it() {
        assert!(!exclusive(
            "(progn (mark 1) (loop for x in xs do (mark 2)))"
        ));
    }

    #[test]
    fn nested_exclusion_survives_a_matching_outer_arm() {
        assert!(exclusive("(if p (if q (mark 1) (mark 2)) nil)"));
    }

    #[test]
    fn a_pruned_walk_stops_below_the_node_it_refuses_to_descend() {
        let parsed = tree("(a (declare (type hash-table h)) (b))");
        let mut heads = Vec::new();
        for_each_evaluated_subview_with_branches(&parsed.root_view(), |view, _| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
            list_head(view).is_none_or(|head| head != "declare")
        });
        assert_eq!(heads, vec!["a", "declare", "b"]);
    }
}
