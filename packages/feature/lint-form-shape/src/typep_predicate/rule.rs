//! `typep-predicate`: a typep against a type with a dedicated predicate ((typep x 'string) is (stringp x)).
//!
//! The analysis lives in [`crate::typep_predicate::domain`], which also backs the
//! standalone `inspect typep-predicate` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::typep_predicate::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "typep-predicate",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a typep against a type with a dedicated predicate ((typep x 'string) is (stringp x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("typep")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut typep_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut typep_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (typep x 'TYPE) is (PRED x).
                let text = format!("({} {})", item.predicate, context_slice(item.object_span));

                RuleFix::single(
                    item.span,
                    text,
                    format!("Rewrite typep as ({} x)", item.predicate),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "typep against this type has a dedicated predicate; use ({} x)",
                    item.predicate
                ),
                fix,
            );
        }
        Ok(())
    }
}
