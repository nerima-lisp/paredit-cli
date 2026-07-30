//! `quoted-case-key`: a case/ecase/ccase clause with a quoted key ('a matches quote and a, not a).
//!
//! The analysis lives in [`crate::quoted_case_key::domain`], which also backs the
//! standalone `inspect quoted-case-key` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::quoted_case_key::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "quoted-case-key",
    RuleCategory::Suspicious,
    Severity::Error,
    "a case/ecase/ccase clause with a quoted key ('a matches quote and a, not a)",
    Fixability::ReportOnly,
);

/// Every head `examine_case` accepts: the `eql`-key `case` family (`typecase`
/// tests type specifiers, a different shape, and is not covered here).
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("ccase"),
    NormalizedHead::new("ecase"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} key {} is quoted; case keys are not evaluated",
                    item.head, item.key
                ),
            );
        }
        Ok(())
    }
}
