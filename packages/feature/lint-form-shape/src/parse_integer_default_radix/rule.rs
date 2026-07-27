//! `parse-integer-default-radix`: a parse-integer call with an explicit :radix 10, the default ((parse-integer s :radix 10) is (parse-integer s)).
//!
//! The analysis lives in [`crate::parse_integer_default_radix::domain`], which also backs the
//! standalone `inspect parse-integer-default-radix` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::parse_integer_default_radix::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::value::evaluate_constant;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "parse-integer-default-radix",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a parse-integer call with an explicit :radix 10, the default ((parse-integer s :radix 10) is (parse-integer s))",
    Fixability::Fixable,
);

/// The only head `examine` accepts.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("parse-integer")];

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
        // `#xA` and a constant bound to ten are the same redundant argument as
        // a literal `10`; `Unknown` reads as "not provably ten", so a radix
        // computed at run time is left alone.
        let is_ten = |radix: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                radix,
                context.binding_table(),
                context.value_table(),
            )
            .is_integer(10)
        };

        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &is_ten,
            &mut call_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant :radix 10".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit :radix 10 restates parse-integer's default; drop it".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
