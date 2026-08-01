//! `leftover-format-debug-marker`: a (format t ...) whose control string carries a DEBUG/DBG marker.
//!
//! The analysis lives in [`crate::leftover_format_debug_marker::domain`], which also backs the
//! standalone `inspect leftover-format-debug-marker` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_format_debug_marker::domain::examine;
use crate::support::OperatorScope;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "leftover-format-debug-marker",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (format t ...) whose control string carries a DEBUG/DBG marker",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context.binding_table());
        examine(
            view,
            &scope,
            context.path(),
            &mut scanned_form_count,
            &mut items,
        );
        for item in items {
            let message = "format's control string carries a DEBUG/DBG marker".to_owned();
            match item.fix_span {
                Some(fix_span) => {
                    let fix = RuleFix::single(
                        fix_span,
                        String::new(),
                        "Remove the leftover debug format call".to_owned(),
                    );
                    sink.report_fixed(item.span, message, fix);
                }
                None => sink.report(item.span, message),
            }
        }
        Ok(())
    }
}
