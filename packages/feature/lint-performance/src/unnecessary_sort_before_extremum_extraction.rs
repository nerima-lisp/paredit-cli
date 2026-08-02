//! `unnecessary-sort-before-extremum-extraction`: ordering a whole sequence to
//! read one end of it.
//!
//! `(first (sort xs #'<))` orders every element of `xs` — O(n log n) comparisons
//! — and then reads one. `(reduce #'min xs)` answers the same question in one
//! linear pass with no ordering at all, and `(loop for x in xs minimize x)`
//! spells it without the closure.
//!
//! # Why the accessor is the anchor, not `sort`
//!
//! The finding is a *pair*, and only the accessor can see both halves of it: an
//! anchor on `sort` would be handed the inner call with no way to learn what
//! consumes it, because [`RuleContext`] carries no parent pointer and recovering
//! one per `sort` would mean an ancestor walk from every sort in the file. The
//! same reasoning produced `lint-sequence`'s `car-reverse`, which anchors on
//! `car`/`first` to find an inner `reverse`; this rule is that rule's shape with
//! a different inner operator, and the two cannot both fire.
//!
//! # Why direct nesting is the whole guard
//!
//! Sorting is only wasteful here if the ordered sequence is otherwise thrown
//! away, and that is exactly what the direct nesting establishes. A program that
//! wants the ordering as well has to *name* it —
//!
//! ```lisp
//! (let ((sorted (sort xs #'<)))
//!   (list (first sorted) (car (last sorted))))
//! ```
//!
//! — and then the accessor's argument is a symbol, not a `sort` call, and this
//! rule says nothing. No dataflow analysis is needed to reach that conclusion,
//! and none is performed.
//!
//! # Deliberately not reported
//!
//! - Any accessor that takes more than one element. `(subseq (sort xs #'<) 0 3)`
//!   and `(last (sort xs #'<) 3)` are top-k queries, for which sorting is a
//!   reasonable implementation.
//! - `(second (sort …))`, `(nth 2 (sort …))` and friends, for the same reason:
//!   an order statistic that is not an extremum has no one-pass spelling worth
//!   suggesting.
//!
//! # Rules that can fire on the same form
//!
//! - **`sort-not-guaranteed-stable`** (`lint-portability`, added by a sibling
//!   batch) anchors on `sort` and complains about *which* equal element comes
//!   back. `(first (sort entries #'< :key #'priority))` earns both findings, and
//!   correctly so: the two say different things — that the ordering is not
//!   needed at all, and that if it is needed it is not reproducible. Neither
//!   subsumes the other, and the fix for one is not the fix for the other.
//! - **`destructive-literal`** (`lint-sequence`) lists `sort` among its heads
//!   and fires when the sequence is a quoted literal, so
//!   `(first (sort '(3 1 2) #'<))` earns both. That is two complaints about one
//!   form, on unrelated grounds.
//!
//! Report-only, for two reasons. The replacement depends on what the comparator
//! means — `(reduce #'min …)` is right for `#'<` on numbers and wrong for a
//! `:key`ed comparison of structures, where `(reduce (lambda (a b) (if (< (k a)
//! (k b)) a b)) …)` is the honest form. And `sort` is destructive: a program that
//! passes `xs` directly is also permuting `xs`, which a linear pass would stop
//! doing. Both are decisions about the program.
//!
//! Scope: Common Lisp only.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::shared::{symbol_in, unqualified};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "unnecessary-sort-before-extremum-extraction",
    RuleCategory::Performance,
    Severity::Warning,
    "a full sort whose result is only read at one end, where a single linear pass would do",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Sorting orders every element — O(n log n) comparisons — to answer a question about one \
         of them. Finding a minimum or a maximum is a single linear pass with no ordering, and \
         `reduce`, `loop … minimize`, and `loop … maximize` all spell it directly.",
    )
    .with_example("(first (sort scores #'<))", "(reduce #'min scores)")
    .with_caveat(
        "Only a directly nested sort is read. A program that also wants the ordering has to bind \
         it — `(let ((sorted (sort xs #'<))) …)` — and then this rule says nothing, which is how \
         \"the ordering is otherwise discarded\" is established without any dataflow analysis. \
         Accessors that take more than one element are left alone: `(subseq (sort xs #'<) 0 3)` \
         is a top-k query, for which sorting is a reasonable answer.",
    ),
);

/// The single-element accessors, paired with which end they read.
///
/// `last` is included even though it returns the last *cons* rather than the
/// last element: `(car (last (sort xs #'<)))` is the idiom, and the waste is the
/// same either way.
///
/// Everything wider is absent on purpose. `second`, `nth`, and `subseq` are
/// order statistics or prefixes, and sorting is a defensible way to compute
/// those.
const ACCESSORS: [(&str, &str); 3] = [("car", "first"), ("first", "first"), ("last", "last")];

/// The two sorting operators. `stable-sort` is included because the stability is
/// irrelevant when only one end is read.
const SORTS: [&str; 2] = ["sort", "stable-sort"];

/// One sort whose ordering is read at one end and then discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortedExtremum {
    /// The whole accessor call, which is what is reported.
    pub span: ByteSpan,
    /// The accessor, as written.
    pub accessor: String,
    /// The sorting operator, as written.
    pub sort: String,
    /// Which end of the ordering is read.
    pub end: &'static str,
}

/// Reads one accessor call and reports the sort it consumes, if any.
///
/// Everything read is inside the matched node. No ancestor, no sibling, no
/// whole-file scan.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<SortedExtremum> {
    let head = list_head(view)?;
    let (accessor, end) = ACCESSORS
        .iter()
        .find(|(name, _)| symbol_in(head, &[name]))
        .copied()?;
    // Exactly one argument. `(last xs 3)` asks for three elements, and sorting
    // to get them is not this rule's complaint.
    if view.children.len() != 2 {
        return None;
    }
    let argument = &view.children[1];
    // `'(sort xs)` is a literal list, not a call.
    if !argument.reader_prefixes.is_empty() {
        return None;
    }
    let sort_head = list_head(argument)?;
    if !symbol_in(sort_head, &SORTS) || argument.children.len() < 2 {
        return None;
    }
    Some(SortedExtremum {
        span: view.span,
        accessor: unqualified(accessor).to_ascii_lowercase(),
        sort: unqualified(sort_head).to_ascii_lowercase(),
        end,
    })
}

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("car"),
    NormalizedHead::new("first"),
    NormalizedHead::new("last"),
];

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
        // Only now, with a candidate in hand: the engine's dispatch walks into
        // quoted data, and `'(first (sort xs #'<))` is a list of symbols.
        if is_unevaluated_at(context.tree(), found.span) {
            return Ok(());
        }
        let suggestion = if found.end == "first" {
            "(reduce #'min …) or (loop … minimize …)"
        } else {
            "(reduce #'max …) or (loop … maximize …)"
        };
        sink.report(
            found.span,
            format!(
                "this {} orders the whole sequence to read its {} element and then discards the \
                 ordering; {} answers it in one pass",
                found.sort, found.end, suggestion
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::{messages, reported};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn end_of(input: &str) -> Option<&'static str> {
        examine(&form(input)).map(|found| found.end)
    }

    fn findings(source: &str) -> Vec<String> {
        reported(&META, &RULE, source)
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_taking_the_first_element_of_a_sort() {
        assert_eq!(end_of("(first (sort xs #'<))"), Some("first"));
        assert_eq!(end_of("(car (sort xs #'<))"), Some("first"));
        assert_eq!(findings("(first (sort xs #'<))").len(), 1);
    }

    #[test]
    fn flags_taking_the_last_cons_of_a_sort() {
        assert_eq!(end_of("(last (sort xs #'<))"), Some("last"));
    }

    #[test]
    fn flags_a_stable_sort_the_same_way() {
        // Stability cannot matter when only one end is read.
        assert_eq!(end_of("(first (stable-sort xs #'<))"), Some("first"));
    }

    #[test]
    fn flags_a_sort_of_a_defensive_copy() {
        // Still O(n log n) to answer a one-pass question; the copy is what makes
        // the destructive sort safe and is `unnecessary-copy`'s subject, not
        // this rule's.
        assert_eq!(end_of("(first (sort (copy-list xs) #'<))"), Some("first"));
    }

    #[test]
    fn flags_a_keyed_sort() {
        assert_eq!(
            end_of("(first (sort entries #'< :key #'priority))"),
            Some("first")
        );
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(end_of("(cl:first (cl:sort xs #'<))"), Some("first"));
    }

    #[test]
    fn the_message_names_the_end_that_is_read() {
        let first = messages(&META, &RULE, "(first (sort xs #'<))");
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("minimize"), "{first:?}");
        let last = messages(&META, &RULE, "(last (sort xs #'<))");
        assert!(last[0].contains("maximize"), "{last:?}");
    }

    #[test]
    fn the_reported_span_is_the_accessor_call() {
        assert_eq!(
            findings("(defun best (xs) (first (sort xs #'<)))"),
            vec!["(first (sort xs #'<))"]
        );
    }

    // -- near-miss negatives --------------------------------------------------

    /// The guard that makes "otherwise discarded" true without dataflow: a
    /// program that also wants the ordering has to name it.
    #[test]
    fn does_not_flag_a_sort_whose_result_is_bound_and_used_twice() {
        assert!(
            findings("(let ((sorted (sort xs #'<))) (list (first sorted) (car (last sorted))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_accessor_on_something_that_is_not_a_sort() {
        assert_eq!(end_of("(first xs)"), None);
        assert_eq!(end_of("(first (reverse xs))"), None);
        assert_eq!(end_of("(first (remove-if #'p xs))"), None);
    }

    /// A head that merely starts with `sort` is a different operator.
    #[test]
    fn does_not_flag_a_differently_named_operator() {
        assert_eq!(end_of("(first (sort-by #'priority xs))"), None);
        assert_eq!(end_of("(first (sorted xs))"), None);
    }

    /// Top-k, not an extremum: sorting is a defensible way to get three
    /// elements.
    #[test]
    fn does_not_flag_an_accessor_that_takes_more_than_one_element() {
        assert_eq!(end_of("(last (sort xs #'<) 3)"), None);
        assert!(findings("(subseq (sort xs #'<) 0 3)").is_empty());
        assert!(findings("(second (sort xs #'<))").is_empty());
        assert!(findings("(nth 2 (sort xs #'<))").is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_sort_call() {
        assert!(findings("(sort xs #'<)").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_form_in_the_argument_position() {
        assert_eq!(end_of("(first '(sort xs #'<))"), None);
    }

    /// A `sort` with no sequence argument at all is malformed, and a malformed
    /// call is `accessor-arity`'s finding rather than a cost claim.
    #[test]
    fn does_not_flag_a_sort_with_no_arguments() {
        assert_eq!(end_of("(first (sort))"), None);
    }

    // -- quote-context negative -----------------------------------------------

    #[test]
    fn reports_nothing_inside_quoted_data() {
        assert!(findings("'(first (sort xs #'<))").is_empty());
        assert!(findings("(quote (first (sort xs #'<)))").is_empty());
        assert!(findings("`(first (sort xs #'<))").is_empty());
        assert!(findings("(defparameter *template* '(first (sort xs #'<)))").is_empty());
    }

    #[test]
    fn reports_a_form_escaped_back_into_code_by_an_unquote() {
        assert_eq!(findings("`(a ,(first (sort xs #'<)))").len(), 1);
    }

    // -- string-literal negative ----------------------------------------------

    #[test]
    fn reports_nothing_spelled_only_inside_a_string() {
        assert!(findings("(format t \"(first (sort xs #'<))\")").is_empty());
        assert!(findings("(defun doc () \"(first (sort xs #'<))\")").is_empty());
    }
}
