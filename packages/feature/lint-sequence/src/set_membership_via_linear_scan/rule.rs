//! `set-membership-via-linear-scan`: a member against a long literal list of symbols, which is a set in disguise.
//!

use paredit_core_lint_engine::LintResult;

use crate::set_membership_via_linear_scan::domain::{MIN_ELEMENTS, examine, message_for};
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "set-membership-via-linear-scan",
    RuleCategory::Performance,
    Severity::Warning,
    "a member against a long literal list of symbols, which is a set in disguise",
    Fixability::ReportOnly,
)
.with_settings(&[MIN_ELEMENTS])
.with_explanation(
    RuleExplanation::new(
        "`member` walks its list, so a membership test against a fixed set of names costs the \
         length of that set on every call. Past a certain size the list has stopped being an \
         argument and become a set, and both a `case` and a hash table answer in constant time.",
    )
    .with_example(
        "(member key '(alpha beta gamma delta epsilon zeta eta theta))",
        "(case key ((alpha beta gamma delta epsilon zeta eta theta) t))",
    )
    .with_caveat(
        "Only a three-operand call over a literal list of plain symbols is read, and only past \
         `min-elements` distinct names (eight by default), so an ordinary two- or three-way test \
         is never reported. A list holding a string, a number or a sublist is left to \
         `eql-search-literal`. `case` yields its clause's value where `member` yields the tail it \
         found, so the rewrite preserves meaning only where the result is used as a boolean.",
    ),
);

/// One head, and not a dense one.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("member")];

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
    ) -> LintResult<()> {
        let threshold = context.setting(META.name().as_str(), MIN_ELEMENTS);
        let Ok(threshold) = usize::try_from(threshold) else {
            // A negative threshold would report every `member`; the knob is a
            // count, so anything below zero is read as "off".
            return Ok(());
        };
        let mut member_form_count = 0;
        let mut items = Vec::new();
        examine(view, threshold, &mut member_form_count, &mut items);
        for item in items {
            // The *call* must be code. Its list argument is deliberately data —
            // see the domain module's note on the inverted polarity.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            sink.report(item.span, message_for(item.distinct));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_lint_engine::policy::RuleDialectScope;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn is_report_only_and_common_lisp_scoped() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
        assert_eq!(META.severity(), Severity::Warning);
        assert_eq!(META.category(), RuleCategory::Performance);
        assert_eq!(RULE.dialect_scope(), RuleDialectScope::COMMON_LISP_ONLY);
        assert!(!RULE.dialect_scope().includes(Dialect::Clojure));
    }

    #[test]
    fn the_head_filter_is_not_a_whole_tree_walk() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
    }

    /// The knob has to be declared on the metadata, or `--rule-arg` rejects it
    /// before a file is ever read.
    #[test]
    fn the_threshold_is_a_declared_setting() {
        assert_eq!(
            META.setting("min-elements")
                .expect("declared knob")
                .default(),
            8
        );
        assert_eq!(META.setting("max"), None);
    }
}
