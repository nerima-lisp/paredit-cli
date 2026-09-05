//! `maphash-mutates-other-entry`: a `maphash` body that adds or removes an
//! entry other than the one being processed.
//!

use paredit_core_lint_engine::LintResult;

use crate::maphash_mutates_other_entry::domain::examine_maphash_mutates_other_entry;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "maphash-mutates-other-entry",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a maphash body that adds or removes an entry other than the one being processed",
    // No fix. The repair is to collect the affected keys during the walk and
    // act on them after it, which means introducing an accumulator and moving
    // the call — a restructuring, not a substitution.
    Fixability::ReportOnly,
);

/// `examine_maphash_mutates_other_entry` only ever matches a `maphash` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("maphash")];

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
        let mut maphash_form_count = 0;
        let mut items = Vec::new();
        examine_maphash_mutates_other_entry(
            context.tree(),
            view,
            &mut maphash_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
