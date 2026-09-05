//! `defstruct-include-type-mismatch`: a `defstruct` whose `:include` names a
//! same-file structure with a different `:type`.
//!

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::defstruct_include_type_mismatch::domain::examine_defstruct_include_type_mismatch;

pub const META: RuleMeta = RuleMeta::new(
    "defstruct-include-type-mismatch",
    RuleCategory::Malformed,
    // SBCL refuses the pair, so neither structure is defined; this is a file
    // that does not load, not a style preference.
    Severity::Error,
    "a defstruct whose :include names a structure declared with a different :type",
    // No fix. Making the two agree means changing one of them, and which one
    // is wrong depends on what the representation is for — a `:type list`
    // parent is often deliberate and the child is the mistake, or the reverse.
    Fixability::ReportOnly,
);

/// `examine_defstruct_include_type_mismatch` only ever matches a `defstruct`.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defstruct")];

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
        let mut defstruct_form_count = 0;
        let mut items = Vec::new();
        examine_defstruct_include_type_mismatch(
            context.tree(),
            view,
            &mut defstruct_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
