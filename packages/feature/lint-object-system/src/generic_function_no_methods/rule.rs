//! `generic-function-no-methods`: a `defgeneric` no `defmethod` in the file
//! ever specializes.
//!
//! The analysis lives in [`crate::generic_function_no_methods::domain`], which
//! also backs the standalone `inspect generic-function-no-methods` command;
//! this module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::generic_function_no_methods::domain::examine_generic_function_no_methods;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "generic-function-no-methods",
    // `ObjectSystem`, not `DeadCode`. Every other `dead-code` member is a form
    // that *cannot execute*; a `defgeneric` with no methods executes fine and
    // installs a generic function. What is wrong is generic/method
    // disagreement, which is verbatim what `ObjectSystem` covers
    // ("`defgeneric`/`defmethod` agreement") and where the sibling rule
    // `method-lambda-list-mismatch` already lives.
    RuleCategory::ObjectSystem,
    // Warning, and it is also the floor: `Severity` has no level below it. The
    // cross-file blind spot below argues for the lowest severity available,
    // which this is.
    Severity::Warning,
    "a defgeneric no defmethod in the file ever specializes",
    // No fix. The repair is either to write the missing method or to delete the
    // declaration, and those are opposite intentions.
    Fixability::ReportOnly,
);

/// `examine_generic_function_no_methods` only ever matches a `defgeneric` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defgeneric")];

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
        let mut defgeneric_form_count = 0;
        let mut items = Vec::new();
        examine_generic_function_no_methods(
            context.tree(),
            view,
            &mut defgeneric_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "no defmethod in this file specializes generic function {}: if its methods \
                     are not defined in another file, it has nothing to dispatch to",
                    item.generic
                ),
            );
        }
        Ok(())
    }
}
