//! `modify-macro-arity`: an incf/decf/push/pop call with the wrong number of arguments.
//!

use paredit_core_lint_engine::LintResult;

use crate::modify_macro_arity::domain::examine_call;
use crate::modify_macro_arity::domain::expected_arity_phrase;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "modify-macro-arity",
    RuleCategory::Arity,
    Severity::Error,
    "an incf/decf/push/pop call with the wrong number of arguments",
    Fixability::ReportOnly,
);

/// Every head `examine_call` accepts: the fixed-arity place-modifying macros.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("incf"),
    NormalizedHead::new("decf"),
    NormalizedHead::new("push"),
    NormalizedHead::new("pop"),
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut call_count = 0;
        let mut items = Vec::new();
        examine_call(view, &mut call_count, &mut items);
        for item in items {
            let span = item.span;
            let expected = expected_arity_phrase(&item);

            sink.report(
                span,
                format!(
                    "{} takes {} argument(s) but has {}",
                    item.operator, expected, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
