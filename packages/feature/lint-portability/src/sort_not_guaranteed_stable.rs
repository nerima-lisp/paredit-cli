//! `sort-not-guaranteed-stable`: a multi-pass sort built on `sort`.
//!
//! CLHS says it outright: "The sorting operation performed by **sort** is not
//! guaranteed stable. Elements considered equal by the *predicate* might or
//! might not stay in their original order." — and, of the other function,
//! "**stable-sort** guarantees stability." Whether a given implementation's
//! `sort` happens to be stable is a fact about that implementation, which is
//! why this is a portability rule and not a correctness one: the code works
//! until it is loaded somewhere else.
//!
//! # What is reported, and why only this
//!
//! Plain `sort` is *correct* whenever stability is irrelevant, which is almost
//! every use of it. A rule that reported `sort` would report the idiom, not the
//! bug. So the trigger is the one shape where the source itself says stability
//! is being relied on: sorting a sequence that was *just sorted by a different
//! ordering*.
//!
//! ```text
//! (sort (sort records #'string< :key #'last-name) #'string< :key #'department)
//! ```
//!
//! That is the classic two-pass multi-key sort, and it produces
//! department-then-name order only if the outer pass preserves the order the
//! inner pass established — that is, only if the outer pass is stable. The
//! outer pass is `sort`. On an implementation whose `sort` is a quicksort, the
//! secondary key is simply lost, silently, and only for some inputs.
//!
//! Only the *outer* call has to be stable, so `(stable-sort (sort …) …)` is
//! correct and is not reported: the rule anchors on `sort` alone, and
//! `stable-sort` never reaches it.
//!
//! # Limits, deliberately
//!
//! - **Only a directly nested sort.** Threading the inner result through a
//!   `let` binding is the same bug and is not reported. Finding it would mean
//!   correlating two forms across a function body, and a per-invocation scan of
//!   that kind is what made two shipped rules 98% of a lint run.
//! - **The two passes must order differently.** `(sort (sort xs #'<) #'<)` is
//!   redundant, not unstable — re-sorting by the same predicate and the same
//!   `:key` cannot depend on what the first pass did with ties. Two passes
//!   count as different when either the predicate or the `:key` is written
//!   differently, compared as *shape* so that `#'STRING<` and `#'string<` are
//!   one predicate.
//! - **A sequence with no equal keys** needs no stability, and nothing here can
//!   prove that either way. The rule prefers the false negative: it says
//!   nothing about any `sort` that is not fed another `sort`.
//!
//! Report-only. `stable-sort` is permitted to be slower and to cons where
//! `sort` does not, so trading one for the other is a decision about the
//! program's cost, not a rewrite a rule may make on the author's behalf.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

use crate::support::{is_unevaluated_at, same_shape};

pub const META: RuleMeta = RuleMeta::new(
    "sort-not-guaranteed-stable",
    RuleCategory::Portability,
    Severity::Warning,
    "a sequence sorted twice by different orderings, where the outer pass is the unstable `sort`",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "The standard says `sort` is not guaranteed stable and that `stable-sort` is, so whether \
         equal elements keep their relative order is a property of the implementation. Sorting an \
         already-sorted sequence by a second ordering is the one shape that depends on that \
         order being kept: the outer pass must be stable or the first key is discarded.",
    )
    .with_example(
        "(sort (sort xs #'string< :key #'name) #'string< :key #'dept)",
        "(stable-sort (sort xs #'string< :key #'name) #'string< :key #'dept)",
    )
    .with_caveat(
        "Only a directly nested sort is reported, and only when the two passes order differently. \
         A plain `sort` — including one whose sequence has no equal keys, which needs no \
         stability at all — is never reported.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("sort")];

/// The heads that count as an inner sorting pass. `stable-sort` is included
/// because an inner *stable* pass still establishes an order the outer pass has
/// to preserve — it is the outer call's stability the rule is about.
const SORTING_HEADS: [&str; 2] = ["sort", "stable-sort"];

/// The ordering one sorting call applies: its predicate and its `:key`.
#[derive(Debug, Clone, Copy)]
struct Ordering<'a> {
    predicate: &'a ExpressionView,
    key: Option<&'a ExpressionView>,
}

/// One `sort` whose sequence is another sort's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainedSort {
    pub span: ByteSpan,
    /// How the inner pass was spelled, for the message: `sort` or
    /// `stable-sort`.
    pub inner_head: String,
}

/// The value of a `&key` argument, read from the keyword tail.
///
/// `sort`'s lambda list is `(sequence predicate &key key)`, so the tail starts
/// at index 3 and is keyword/value pairs. Stepping two at a time is what keeps
/// a *value* that happens to be the symbol `:key` from being read as a keyword.
fn keyword_argument<'a>(view: &'a ExpressionView, name: &str) -> Option<&'a ExpressionView> {
    let mut index = 3;
    while index + 1 < view.children.len() {
        if atom_text(&view.children[index]).is_some_and(|text| text.eq_ignore_ascii_case(name)) {
            return view.children.get(index + 1);
        }
        index += 2;
    }
    None
}

/// The ordering a sorting call applies, or `None` if it is not a sorting call
/// with a predicate.
///
/// A `sort` written without a predicate is malformed rather than unstable, and
/// is left to the arity rules.
fn ordering_of<'a>(view: &'a ExpressionView, heads: &[&str]) -> Option<Ordering<'a>> {
    if !list_head(view).is_some_and(|head| symbol_in(head, heads)) {
        return None;
    }
    Some(Ordering {
        predicate: view.children.get(2)?,
        key: keyword_argument(view, ":key"),
    })
}

/// Whether two passes impose different orderings, and so compose only if the
/// second is stable.
fn orders_differently(outer: Ordering<'_>, inner: Ordering<'_>) -> bool {
    if !same_shape(outer.predicate, inner.predicate) {
        return true;
    }
    match (outer.key, inner.key) {
        (Some(one), Some(other)) => !same_shape(one, other),
        (None, None) => false,
        _ => true,
    }
}

/// Reads one `sort` call and reports the sort it is stacked on.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<ChainedSort> {
    // The dispatcher only routes `sort` here, but the check belongs in the
    // function too: a caller reading `examine` alone must not be told that
    // `(stable-sort (sort …) …)` — which is correct — is a finding. That is
    // what the `&["sort"]` argument does, and it is the *only* place it is
    // done: a separate `list_head` pre-check was here and the mutation harness
    // proved it dead, since this call already rejects every other head.
    let outer = ordering_of(view, &["sort"])?;
    let sequence = view.children.get(1)?;
    let inner = ordering_of(sequence, &SORTING_HEADS)?;
    if !orders_differently(outer, inner) {
        return None;
    }
    Some(ChainedSort {
        span: view.span,
        inner_head: list_head(sequence)?.to_owned(),
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        // Asked only once a finding already exists, never per visited node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "this re-sorts the result of an inner {} by a different ordering, which keeps the \
                 inner key only if the outer pass is stable; the standard does not guarantee sort \
                 is stable — use stable-sort for the outer pass",
                found.inner_head
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn finding(input: &str) -> Option<ChainedSort> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
    }

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// How many findings the *real* dispatch produces for `input`.
    ///
    /// `examine` deliberately does not carry the quote guard, so only a run
    /// through the engine exercises it — and this also covers the two
    /// declarations a domain test cannot see: the head filter and the dialect
    /// scope.
    fn reports(input: &str) -> usize {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            std::path::Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_two_pass_sort_by_different_keys() {
        let found = finding("(sort (sort xs #'string< :key #'name) #'string< :key #'dept)")
            .expect("a finding");
        assert_eq!(found.inner_head, "sort");
        assert_eq!(
            reports("(sort (sort xs #'string< :key #'name) #'string< :key #'dept)"),
            1
        );
    }

    #[test]
    fn flags_a_two_pass_sort_by_different_predicates() {
        assert!(finding("(sort (sort xs #'<) #'>)").is_some());
    }

    #[test]
    fn flags_an_outer_sort_over_an_inner_stable_sort() {
        // Only the outer pass has to be stable, and it is not.
        let found = finding("(sort (stable-sort xs #'< :key #'a) #'< :key #'b)").expect("finding");
        assert_eq!(found.inner_head, "stable-sort");
    }

    #[test]
    fn flags_a_key_added_by_only_one_pass() {
        assert!(finding("(sort (sort xs #'< :key #'first) #'<)").is_some());
        assert!(finding("(sort (sort xs #'<) #'< :key #'first)").is_some());
    }

    #[test]
    fn reads_the_heads_case_insensitively_and_past_a_package_prefix() {
        assert!(finding("(CL:SORT (cl:sort xs #'<) #'>)").is_some());
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_plain_sort() {
        assert_eq!(finding("(sort xs #'<)"), None);
        assert_eq!(finding("(sort (copy-seq xs) #'< :key #'name)"), None);
        assert_eq!(finding("(sort (remove-if #'null xs) #'<)"), None);
    }

    #[test]
    fn does_not_flag_a_stable_outer_pass() {
        // The head filter never routes `stable-sort` here, and `examine` agrees.
        assert_eq!(
            finding("(stable-sort (sort xs #'< :key #'a) #'< :key #'b)"),
            None
        );
    }

    #[test]
    fn does_not_flag_two_passes_with_the_same_ordering() {
        // Redundant, but not a stability question: re-sorting by the same
        // predicate and key cannot depend on what the first pass did with ties.
        assert_eq!(finding("(sort (sort xs #'<) #'<)"), None);
        assert_eq!(
            finding("(sort (sort xs #'< :key #'first) #'< :key #'FIRST)"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_sort_without_a_predicate() {
        assert_eq!(finding("(sort (sort xs #'<))"), None);
        assert_eq!(finding("(sort)"), None);
    }

    #[test]
    fn does_not_read_a_key_valued_keyword_as_the_key_argument() {
        // `:key` here is the *value* of `:test`, not a `:key` argument. The
        // inner call has an odd keyword *before* it and a real argument after
        // it, so a scan that stepped one at a time would read `:from-end` as
        // the inner `:key`, disagree with the outer call's absent one, and
        // report. Stepping in pairs never lands on it.
        //
        // The symmetric spelling — the same trap on both passes — was here
        // first and the mutation harness proved it non-discriminating: both
        // sides got the same wrong answer, so the comparison still matched.
        assert_eq!(
            finding("(sort (sort xs #'< :test :key :from-end t) #'<)"),
            None
        );
        assert_eq!(
            finding("(sort (sort xs #'<) #'< :test :key :from-end t)"),
            None
        );
    }

    // -- quote-context negative ----------------------------------------------

    #[test]
    fn does_not_flag_a_sort_in_quoted_data() {
        assert_eq!(reports("'(sort (sort xs #'<) #'>)"), 0);
        assert_eq!(reports("(quote (sort (sort xs #'<) #'>))"), 0);
        assert_eq!(reports("`(sort (sort xs #'<) #'>)"), 0);
        assert_eq!(reports("'(a ,(sort (sort xs #'<) #'>))"), 0);
        assert_eq!(reports("'(outer (sort (sort xs #'<) #'>))"), 0);
    }

    #[test]
    fn flags_a_sort_unquoted_back_into_code() {
        assert_eq!(reports("`(a ,(sort (sort xs #'<) #'>))"), 1);
    }

    // -- string-literal negative ---------------------------------------------

    #[test]
    fn does_not_flag_a_sort_written_inside_a_string() {
        assert_eq!(reports(r#"(format nil "(sort (sort xs #'<) #'>)")"#), 0);
        assert_eq!(
            reports(r#"(defun f () "sorts via (sort (sort xs #'<) #'>)" nil)"#),
            0
        );
    }
}
