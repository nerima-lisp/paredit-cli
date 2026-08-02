//! `intern-dynamic-package-target`: an `intern` whose package argument is
//! computed.
//!
//! The analysis lives in [`crate::intern_dynamic_package_target::domain`],
//! which also backs the standalone `inspect intern-dynamic-package-target`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::intern_dynamic_package_target::domain::examine;

pub const META: RuleMeta = RuleMeta::new(
    "intern-dynamic-package-target",
    // The category's own definition names `intern` outright: this is data
    // reaching the symbol table, and the package it reaches is the part no
    // static search can follow.
    RuleCategory::Security,
    Severity::Warning,
    "an intern whose package argument is a computed expression, so the target package is not \
     statically knowable",
    // No fix. The remedy is either a literal package designator or a lookup
    // keyed by a validated name, and which of the two applies is a design
    // decision about where the symbol is supposed to live.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A symbol interned into a package chosen at run time is not where any cross-reference, \
         grep, or `apropos` will look for it, and a computed package name that names no package \
         makes `find-package` return nil, which `intern` then signals on.",
    )
    .with_example(
        "(intern \"HANDLER\" (find-package (format nil \"APP/~A\" module)))",
        "(intern \"HANDLER\" (or (find-package module) (error 'unknown-module :name module)))",
    )
    .with_caveat(
        "Only a package argument that itself *chooses* a package is reported, and only when the \
         symbol name is a string literal. `(intern \"X\" package)` and \
         `(intern \"SETTER\" (symbol-package sym))` both name a package the caller supplied, and \
         `(intern name pkg)` is `eval-of-non-constant`'s finding rather than this one's.",
    ),
);

/// `examine` only ever matches an `intern` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("intern")];

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
        if let Some(item) = examine(context.tree(), view) {
            sink.report(
                item.span,
                format!(
                    "intern chooses its package with the computed expression ({} …), so nothing \
                     in the source says which package {} is interned into",
                    item.package_operator, item.name
                ),
            );
        }
        Ok(())
    }
}
