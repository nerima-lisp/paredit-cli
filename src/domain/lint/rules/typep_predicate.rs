//! `typep-predicate`: a typep against a type with a dedicated predicate ((typep x 'string) is (stringp x)).
//!
//! The analysis lives in [`crate::domain::typep_predicate_report`], which also backs the
//! standalone `inspect typep-predicate` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::typep_predicate_report::examine;

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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut typep_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut typep_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
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
