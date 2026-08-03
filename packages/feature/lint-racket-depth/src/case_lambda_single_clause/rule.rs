//! `racket-case-lambda-single-clause`: a `case-lambda` that dispatches on
//! nothing.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::case_lambda_single_clause::domain::{DIALECTS, HEAD, MESSAGE, examine_case_lambda};

pub const META: RuleMeta = RuleMeta::new(
    "racket-case-lambda-single-clause",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a case-lambda with one clause, which is a lambda written the long way",
    Fixability::Fixable,
);

const FILTER_HEADS: [NormalizedHead; 1] = [NormalizedHead::new(HEAD)];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILTER_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut case_lambda_form_count = 0;
        let mut items = Vec::new();
        examine_case_lambda(
            context.tree(),
            view,
            &mut case_lambda_form_count,
            &mut items,
        );
        for item in items {
            // The clause's formals and body are copied from source rather than
            // re-printed, so spacing, comments and reader prefixes survive.
            let fix = RuleFix::single(
                item.span,
                format!("(lambda {})", context.slice(item.clause_inner_span)),
                "Rewrite the one-clause case-lambda as a lambda".to_owned(),
            );
            sink.report_fixed(item.span, MESSAGE.to_owned(), fix);
        }
        Ok(())
    }
}
