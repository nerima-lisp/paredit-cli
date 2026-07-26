//! `duplicate-let-bindings`: a parallel let that binds the same variable more than once.
//!
//! The analysis lives in [`crate::domain::duplicate_let_binding_report`], which also backs the
//! standalone `inspect duplicate-let-bindings` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::duplicate_let_binding_report::examine_let;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-let-bindings",
    RuleCategory::Duplicate,
    Severity::Error,
    "a parallel let that binds the same variable more than once",
    Fixability::ReportOnly,
);

/// `let*` binds sequentially, where re-binding a name is legal shadowing, so
/// only plain `let` is examined.
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

            sink.report(
                span,
                format!(
                    "let binds {} more than once ({}×)",
                    item.name, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
