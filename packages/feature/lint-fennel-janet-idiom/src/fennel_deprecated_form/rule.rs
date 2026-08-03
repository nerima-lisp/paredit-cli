//! `fennel-deprecated-form`: a special the Fennel reference deprecates.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::fennel_deprecated_form::domain::{self, examine};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "fennel-deprecated-form",
    RuleCategory::Portability,
    Severity::Warning,
    "a Fennel special the language reference lists under \"Deprecated Forms\"",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Fennel's reference has a \"Deprecated Forms\" section naming three specials and, for \
         each, what replaced it. They still compile, which is exactly why they survive in code \
         long after the replacement landed.",
    )
    .with_example(
        "(require-macros :my.macros)",
        "(import-macros m :my.macros)",
    )
    .with_caveat(
        "A file that deliberately targets a Fennel old enough to lack the replacement will be \
         reported. `import-macros` has been available since 0.4.0 and `global` has been \
         deprecated for longer than that, so the case is narrow, but it is real.",
    ),
);

/// One head per entry of [`domain::DEPRECATIONS`], written out because
/// `NormalizedHead::new` is `const` and the table is not a `const` iterator.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("global"),
    NormalizedHead::new("require-macros"),
    NormalizedHead::new("pick-args"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&domain::DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(item) = examine(context.dialect(), view) else {
            return Ok(());
        };
        // Asked only once a finding exists. The dispatcher hands a rule
        // quoted nodes like any other, so a `(global …)` inside a macro
        // template reaches here — but `is_unevaluated_at` materializes the
        // whole document, so asking before `examine` charges every `global`
        // call in the file for a walk that almost always answers "no".
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "{} is deprecated; use {} ({})",
                item.deprecation.head, item.deprecation.replacement, item.deprecation.citation
            ),
        );
        Ok(())
    }
}
