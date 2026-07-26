//! `constant-when-test`: a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil).
//!
//! The analysis lives in [`crate::domain::constant_when_test_report`], which also backs the
//! standalone `inspect constant-when-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::constant_when_test_report::examine_when;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::{ByteSpan, ExpressionView};

pub const META: RuleMeta = RuleMeta::new(
    "constant-when-test",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("when"), NormalizedHead::new("unless")];

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
        let mut when_form_count = 0;
        let mut items = Vec::new();
        examine_when(view, context.path(), &mut when_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                if item.always_runs {
                    // The body always runs: splice `(when t` / `(unless nil` down to
                    // `(progn`, keeping the body forms verbatim.
                    RuleFix::single(
                        ByteSpan::new(item.span.start(), item.splice_span.end()),
                        "(progn".to_owned(),
                        format!(
                            "Rewrite the always-true ({} {} …) as progn",
                            item.head, item.test
                        ),
                    )
                } else {
                    // The body never runs: the whole form is just nil.
                    RuleFix::single(
                        item.span,
                        "nil".to_owned(),
                        format!("Collapse the dead ({} {} …) to nil", item.head, item.test),
                    )
                }
            };
            let message = if item.always_runs {
                format!(
                    "{} test is the constant {}; the body always runs, so this is a progn",
                    item.head, item.test
                )
            } else {
                format!(
                    "{} test is the constant {}; the body never runs, so this is nil",
                    item.head, item.test
                )
            };

            sink.report_fixed(span, message, fix);
        }
        Ok(())
    }
}
