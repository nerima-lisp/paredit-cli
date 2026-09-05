//! `butlast-default-count`: a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::butlast_default_count::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "butlast-default-count",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("butlast"),
    NormalizedHead::new("nbutlast"),
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
    ) -> LintResult<()> {
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                RuleFix::multi(
                    "Drop the redundant butlast count of 1".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit count of 1 restates butlast's default; (butlast x 1) is (butlast x)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
