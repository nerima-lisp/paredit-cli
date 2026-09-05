//! `scheme-memq-assq-literal-key`: `memq`/`assq` searching for a number or
//! character literal, which R7RS 6.4 leaves unspecified.
//!

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::memq_assq_literal_key::domain::{DIALECTS, HEADS, examine_memq_assq, message_for};

pub const META: RuleMeta = RuleMeta::new(
    "scheme-memq-assq-literal-key",
    RuleCategory::Portability,
    Severity::Warning,
    "memq or assq searching for a number or character literal, which R7RS 6.4 leaves unspecified",
    Fixability::Fixable,
);

/// `examine_memq_assq` only ever matches these two heads.
const HEAD_FILTER: [NormalizedHead; 2] =
    [NormalizedHead::new(HEADS[0]), NormalizedHead::new(HEADS[1])];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEAD_FILTER)
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
        let mut search_form_count = 0;
        let mut items = Vec::new();
        examine_memq_assq(context.tree(), view, &mut search_form_count, &mut items);
        for item in items {
            let message = message_for(&item.head, item.replacement, item.kind);
            let fix = RuleFix::single(
                item.head_span,
                item.replacement.to_owned(),
                format!("Replace {} with {}", item.head, item.replacement),
            );
            sink.report_fixed(item.span, message, fix);
        }
        Ok(())
    }
}
