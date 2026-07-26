//! `funcall-lambda`: a funcall of a lambda form, which applies directly ((funcall (lambda (x) x) a) is ((lambda (x) x) a)).
//!
//! The analysis lives in [`crate::domain::funcall_lambda_report`], which also backs the
//! standalone `inspect funcall-lambda` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::funcall_lambda_report::examine_funcall;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::{ByteSpan, ExpressionView};

pub const META: RuleMeta = RuleMeta::new(
    "funcall-lambda",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a funcall of a lambda form, which applies directly ((funcall (lambda (x) x) a) is ((lambda (x) x) a))",
    Fixability::Fixable,
);

/// `examine_funcall` only ever matches a `funcall` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("funcall")];

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
        let mut funcall_form_count = 0;
        let mut items = Vec::new();
        examine_funcall(view, context.path(), &mut funcall_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Delete `funcall ` from the head up to the lambda form, leaving the
                // lambda in operator position: ((lambda …) …).

                RuleFix::multi(
                    "Apply the lambda directly, dropping funcall".to_owned(),
                    Replacement::new(
                        ByteSpan::new(item.head_span.start(), item.lambda_span.start()),
                        String::new(),
                    ),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "funcall of a lambda applies it directly; ((lambda …) …) drops the funcall"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
