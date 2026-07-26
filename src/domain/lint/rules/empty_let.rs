//! `empty-let`: a let with an empty binding list, which is just progn ((let () body) is (progn body)).
//!
//! The analysis lives in [`crate::domain::empty_let_report`], which also backs the
//! standalone `inspect empty-let` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::empty_let_report::examine_let;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "empty-let",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let with an empty binding list, which is just progn ((let () body) is (progn body))",
    Fixability::Fixable,
);

/// `examine_let` only matches bare `let`, not `let*` (see the module doc).
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("let")];

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
        let mut let_form_count = 0;
        let mut items = Vec::new();
        examine_let(view, context.path(), &mut let_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Replace the `(let ()` prefix with `(progn`, leaving the body intact.

                RuleFix::multi(
                    "Rewrite the empty let as progn".to_owned(),
                    Replacement::new(item.prefix_span, "(progn".to_owned()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "let with no bindings is just progn; (let () body) is (progn body)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
