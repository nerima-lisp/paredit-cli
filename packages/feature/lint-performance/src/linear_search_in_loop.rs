//! `linear-search-in-loop`: a linear scan of the same collection, every round.
//!
//! `(dolist (x xs) (when (member x haystack) …))` is O(|xs| × |haystack|). The
//! standard remedy is to build a hash table of `haystack` once before the loop
//! and look up in constant time.
//!
//! What makes the finding worth making is the word *same*: the collection must
//! not depend on the loop. `(dolist (x xs) (member y x))` searches something
//! different each round and cannot be hoisted, so it is not reported. That
//! check — `crate::shared::mentions_any` against the loop's own variables —
//! is the rule, and everything else is bookkeeping.
//!
//! Report-only: the rewrite introduces a new binding before the loop and
//! changes the search's equality semantics from the sequence function's
//! defaults to the hash table's test, which is a decision, not a mechanical
//! substitution.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

use crate::shared::{ITERATION_HEADS, iteration_variables, mentions_any, symbol_in, unqualified};

/// The knob: how many searches in one loop body it takes before the rule
/// speaks.
///
/// A default of 1 makes the rule fire on the single-search case, which is the
/// common and usually correct finding. A project that has decided one lookup is
/// acceptable and only wants the pathological cases raises it rather than
/// turning the rule off.
pub const MIN_SEARCHES: RuleSetting = RuleSetting::new(
    "min-searches",
    1,
    "how many loop-invariant linear searches a loop body needs before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "linear-search-in-loop",
    RuleCategory::Performance,
    Severity::Warning,
    "a loop that linearly searches the same collection on every round, which is quadratic",
    Fixability::ReportOnly,
)
.with_settings(&[MIN_SEARCHES])
.with_explanation(
    RuleExplanation::new(
        "`member`, `assoc`, `find`, and `position` walk their sequence. Calling one inside a loop \
         on a collection the loop does not change multiplies the two lengths, where hoisting the \
         collection into a hash table before the loop makes each lookup constant.",
    )
    .with_example(
        "(dolist (x xs) (when (member x known) (use x)))",
        "(let ((seen (make-hash-table :test #'equal))) … (dolist (x xs) (when (gethash x seen) (use x))))",
    )
    .with_caveat(
        "A search whose collection mentions a loop variable is different on every round, cannot \
         be hoisted, and is never reported.",
    ),
);

const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("mapc"),
    NormalizedHead::new("maplist"),
];

/// The searches, paired with the argument position their sequence occupies.
///
/// `member`/`assoc`/`find`/`position` take `(item sequence …)`; `gethash` is
/// deliberately absent, being the thing this rule recommends.
const SEARCHES: [(&str, usize); 8] = [
    ("member", 2),
    ("assoc", 2),
    ("rassoc", 2),
    ("find", 2),
    ("position", 2),
    ("search", 2),
    ("count", 2),
    ("intersection", 1),
];

/// One loop-invariant linear search inside a loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantSearch {
    pub span: ByteSpan,
    pub operator: String,
}

/// Every loop-invariant linear search in one iteration form's body.
#[must_use]
pub fn examine_loop(view: &ExpressionView) -> Vec<InvariantSearch> {
    if !list_head(view).is_some_and(|head| symbol_in(head, &ITERATION_HEADS)) {
        return Vec::new();
    }
    let variables = iteration_variables(view);
    if variables.is_empty() {
        // A loop with no readable bindings gives no way to tell invariant from
        // varying, and guessing would report every search in every `loop`.
        return Vec::new();
    }

    let mut found = Vec::new();
    for_each_subview(view, |subview| {
        let Some(head) = list_head(subview) else {
            return;
        };
        let Some((_, position)) = SEARCHES
            .iter()
            .find(|(name, _)| symbol_in(head, &[name]))
            .copied()
        else {
            return;
        };
        let Some(sequence) = subview.children.get(position) else {
            return;
        };
        if mentions_any(sequence, &variables) {
            return;
        }
        found.push(InvariantSearch {
            span: subview.span,
            operator: unqualified(head).to_ascii_lowercase(),
        });
    });
    found
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
        let found = examine_loop(view);
        let threshold = context.setting(META.name().as_str(), MIN_SEARCHES);
        if i64::try_from(found.len()).unwrap_or(i64::MAX) < threshold {
            return Ok(());
        }
        for search in found {
            sink.report(
                search.span,
                format!(
                    "{} searches a collection the loop never changes, once per round; \
                     hoist it into a hash table before the loop",
                    search.operator
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn operators(input: &str) -> Vec<String> {
        examine_loop(&form(input))
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    #[test]
    fn flags_a_search_of_a_collection_the_loop_does_not_change() {
        assert_eq!(
            operators("(dolist (x xs) (when (member x known) (use x)))"),
            vec!["member"]
        );
    }

    #[test]
    fn does_not_flag_a_search_whose_collection_varies_with_the_loop() {
        assert!(operators("(dolist (row rows) (find key row))").is_empty());
    }

    #[test]
    fn reads_the_loop_variable_through_a_nested_collection_expression() {
        assert!(operators("(dolist (row rows) (find key (cdr (entries row))))").is_empty());
    }

    #[test]
    fn flags_every_search_family_member() {
        assert_eq!(operators("(dotimes (i n) (assoc i table))"), vec!["assoc"]);
        assert_eq!(
            operators("(loop for x in xs do (position x haystack))"),
            vec!["position"]
        );
    }

    #[test]
    fn reads_the_sequence_argument_at_the_right_position() {
        // `intersection` takes two sequences and its first is the searched one.
        assert_eq!(
            operators("(dolist (x xs) (intersection base others))"),
            vec!["intersection"]
        );
    }

    #[test]
    fn a_loop_with_no_readable_bindings_reports_nothing() {
        // Without a variable list there is no way to tell invariant from
        // varying, and guessing would flag every search in every `loop`.
        assert!(operators("(loop (member x known))").is_empty());
    }

    #[test]
    fn a_search_outside_any_loop_reports_nothing() {
        assert!(operators("(member x known)").is_empty());
    }

    #[test]
    fn several_searches_in_one_body_are_all_reported() {
        assert_eq!(
            operators("(dolist (x xs) (or (member x as) (assoc x bs)))"),
            vec!["member", "assoc"]
        );
    }
}
