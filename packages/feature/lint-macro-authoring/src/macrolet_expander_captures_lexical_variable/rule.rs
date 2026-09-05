//! `macrolet-expander-captures-lexical-variable`: a `macrolet` expander that
//! evaluates a name an enclosing form binds lexically.
//!

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macrolet_expander_captures_lexical_variable::domain::examine_macrolet_expander_captures_lexical_variable;

pub const META: RuleMeta = RuleMeta::new(
    "macrolet-expander-captures-lexical-variable",
    RuleCategory::Suspicious,
    // CLHS calls the consequences undefined; SBCL ships a fasl and fails at the
    // call, or answers with the global value and never says anything.
    Severity::Error,
    "a macrolet expander evaluating a name bound by an enclosing lexical binding",
    // Both repairs — move the reference into the template, or pass it as an
    // argument — change what the expansion means. Neither is mechanical.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A macrolet expander runs at macroexpansion time, before any enclosing binding exists. \
         CLHS states that the consequences are undefined if a local macro definition references \
         a local variable binding visible in its lexical environment. A name written plainly in \
         the template is fine — it is part of the expansion — but a name the expander evaluates \
         is read out of an environment that is not there yet.",
    )
    .with_example(
        "(let ((n 3)) (macrolet ((rep (f) `(progn ,@(loop repeat n collect f)))) …))",
        "(let ((n 3)) (macrolet ((rep (count f) `(progn ,@(loop repeat count collect f)))) …))",
    )
    .with_caveat(
        "Only variable references are reported. CLHS's sentence also covers local function \
         bindings, which would need the function namespace kept apart from the variable one.",
    ),
);

/// The one form that defines a local macro.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("macrolet")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut macrolet_form_count = 0;
        let mut items = Vec::new();
        examine_macrolet_expander_captures_lexical_variable(
            context.tree(),
            view,
            &mut macrolet_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
