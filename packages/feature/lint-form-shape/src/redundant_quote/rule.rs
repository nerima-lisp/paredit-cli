//! `redundant-quote`: a self-evaluating literal (number/string/char/keyword) quoted redundantly.
//!
//! The analysis lives in [`crate::redundant_quote::domain`], which also backs the
//! standalone `inspect redundant-quote` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_quote::domain::examine_quote;
use crate::support::is_enclosing_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-quote",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a self-evaluating literal (number/string/char/keyword) quoted redundantly",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// A redundant quote is reader sugar on *any* node — `'5` is a quoted atom
    /// — so this cannot be narrowed to a set of operator heads.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::AllNodes
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut quoted_form_count = 0;
        let mut items = Vec::new();
        examine_quote(view, &mut quoted_form_count, &mut items);
        for item in items {
            let span = item.span;
            // The *enclosing* state, not this node's own: the finding here
            // is a quote, so `is_hard_quoted_at` would answer "yes" for
            // every finding this rule can make and silence it entirely.
            // What settles `'5` → `5` is whether something else quotes it —
            // inside `'(a '5)` the inner `'` is a datum, in `(f '5)` it is
            // redundant sugar. Asked once per finding.
            if is_enclosing_hard_quoted_at(context.tree(), span) {
                continue;
            }
            // The replacement is the quoted datum's own source, which the
            // message also names — so it is cloned here rather than moved out
            // of the item, which is the one place in the suite where the fix
            // genuinely needs a copy of a field.
            let fix = RuleFix::single(
                item.span,
                item.literal.clone(),
                "Remove the redundant quote".to_owned(),
            );

            sink.report_fixed(
                span,
                format!("quoting {} {} is redundant", item.kind, item.literal),
                fix,
            );
        }
        Ok(())
    }
}
