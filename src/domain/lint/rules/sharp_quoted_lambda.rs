//! `sharp-quoted-lambda`: a lambda form with a redundant #' prefix (#'(lambda (x) x) is (lambda (x) x)).
//!
//! The analysis lives in [`crate::domain::sharp_quoted_lambda_report`], which also backs the
//! standalone `inspect sharp-quoted-lambda` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::sharp_quoted_lambda_report::examine_lambda;

pub const META: RuleMeta = RuleMeta::new(
    "sharp-quoted-lambda",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a lambda form with a redundant #' prefix (#'(lambda (x) x) is (lambda (x) x))",
    Fixability::Fixable,
);

/// `examine_lambda` only ever matches a `lambda` head (the reader prefix is
/// checked separately, not the head itself).
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("lambda")];

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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut lambda_form_count = 0;
        let mut items = Vec::new();
        examine_lambda(view, context.path(), &mut lambda_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Strip the leading `#'` (tolerating rare whitespace) from the form.
                let whole = context_slice(item.span);
                let text = whole
                    .strip_prefix("#'")
                    .unwrap_or(whole.as_str())
                    .trim_start()
                    .to_owned();

                RuleFix::single(
                    item.span,
                    text,
                    "Drop the redundant #' before lambda".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "#' on a lambda is redundant; #'(lambda …) is (lambda …)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
