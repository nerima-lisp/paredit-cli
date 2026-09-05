//! `method-qualifier-typo`: a `defmethod` qualifier outside `:before`,
//! `:after` and `:around`.
//!

use paredit_core_lint_engine::LintResult;

use crate::method_qualifier_typo::domain::examine_method_qualifier_typo;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "method-qualifier-typo",
    // `ObjectSystem`, matching `lint-convention`'s `defclass-slot-option`: both
    // validate a CLOS definition form against a closed vocabulary.
    RuleCategory::ObjectSystem,
    // A warning, not an error. Standard method combination does signal on an
    // unknown qualifier — but this rule cannot tell that standard combination
    // is in force, because the `define-method-combination` or
    // `(:method-combination …)` that licenses the qualifier is routinely in
    // another file. The domain's whole-file exemption narrows that blind spot;
    // it does not close it, and `Error` would claim it had.
    Severity::Warning,
    "a defmethod qualifier outside :before, :after and :around",
    // No fix. `:arround` is *probably* `:around`, but a qualifier is also how a
    // custom method combination is selected, and rewriting one silently moves
    // the method into a different part of the combination.
    Fixability::ReportOnly,
);

/// `examine_method_qualifier_typo` only ever matches a `defmethod` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

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
        let mut qualifier_count = 0;
        let mut items = Vec::new();
        examine_method_qualifier_typo(context.tree(), view, &mut qualifier_count, &mut items);
        for item in items {
            sink.report(
                item.span,
                format!(
                    "{} is not a standard method qualifier on {}: standard method combination \
                     defines only :before, :after and :around",
                    item.qualifier, item.generic
                ),
            );
        }
        Ok(())
    }
}
