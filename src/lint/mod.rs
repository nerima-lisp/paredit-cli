//! The lint context: what a rule is, how rules are registered, and the single
//! pass that runs them.
//!
//! The suite used to be four hand-maintained parallel arrays plus a 1,650-line
//! function that called 134 collectors in sequence, each walking the whole
//! document on its own, with the matching automatic fixes stranded in the CLI
//! layer. This module inverts that: a rule declares which nodes it wants and
//! what to say about one, [`registry`] is the single list everything else is
//! derived from, and [`engine`] walks the document once for all of them.
//!
//! Layout follows the usual split — [`model`] for the vocabulary, [`policy`]
//! for decisions that need no tree, [`engine`] for the pass, [`registry`] for
//! the catalogue, and [`rules`] for the rules themselves.

// Phase 2 facade (section 4.1). engine/model/policy/rule now live in
// `paredit-core-lint-engine`; `registry` and `rules` stay here, because a
// registry naming every rule while every rule depends on the engine is a cycle
// (section 4.2).
pub use paredit_core_lint_engine::{LintResult, RuleSelectionError, engine, model, policy, rule};

#[cfg(test)]
mod engine_dispatch_tests;
pub mod registry;
pub mod report;
#[cfg(test)]
mod rule_registry_tests;

use std::path::Path;
use std::sync::OnceLock;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use engine::{HeadIndex, PassOptions, PassOutcome};
use model::{LintFinding, LintOutcome, LintPolicy, LintPolicyOptions, LintSummary, Severity};
use policy::{RuleFilter, RuleSelection, SeverityOverrides};
use rule::RuleCatalog;

/// The shipped rule set. The engine and policy take this as an argument, so
/// they never name a rule or count them - that separation is what lets the
/// engine live in its own package while the registry stays with the rules
/// (section 4.2).
pub const CATALOG: RuleCatalog = RuleCatalog::new(&registry::REGISTRY);

/// The process-wide operator index. Building it touches every registered rule,
/// so it is cached here, next to the registry it is derived from, rather than
/// inside the engine.
pub fn head_index() -> &'static HeadIndex {
    static INDEX: OnceLock<HeadIndex> = OnceLock::new();
    INDEX.get_or_init(|| engine::build_head_index(CATALOG))
}

/// Runs the shipped rule set over one file. Thin wrapper over
/// [`engine::collect_lint_outcomes`] that supplies the catalogue and index.
pub fn collect_lint_outcomes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    selection: RuleSelection<'_>,
) -> LintResult<Vec<LintOutcome>> {
    engine::collect_lint_outcomes(
        CATALOG,
        head_index(),
        path,
        dialect,
        tree,
        source,
        selection,
    )
}

/// Runs the shipped rule set over one file with the per-run knobs (rule
/// settings, per-rule timing).
pub fn collect_lint_pass(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    selection: RuleSelection<'_>,
    options: PassOptions<'_>,
) -> LintResult<PassOutcome> {
    engine::collect_lint_pass(
        CATALOG,
        head_index(),
        path,
        dialect,
        tree,
        source,
        selection,
        options,
    )
}

/// The rules `filter` selects, over the shipped set — widened by
/// `custom_rules`, the names a project's own loaded `defrule` files
/// contributed to this run.
pub fn resolve_active_rules(
    filter: &RuleFilter<'_>,
    custom_rules: &[&'static str],
) -> std::result::Result<Vec<&'static str>, RuleSelectionError> {
    policy::resolve_active_rules(CATALOG, filter, custom_rules)
}

/// Applies one `--deny`/`--warn` selector against the shipped set, or against
/// `custom_rules` when the selector names one of them instead.
pub fn apply_severity_override(
    overrides: &mut SeverityOverrides,
    selector: &str,
    severity: Severity,
    custom_rules: &[&'static str],
) -> std::result::Result<(), RuleSelectionError> {
    overrides.apply(CATALOG, selector, severity, custom_rules)
}

/// The severity `rule` is reported at under `overrides`, over the shipped set.
#[must_use]
pub fn overridden_rule_severity(overrides: &SeverityOverrides, rule: &str) -> Severity {
    overrides.severity_of(CATALOG, rule)
}

/// Summarizes findings over the shipped rule set.
#[must_use]
pub fn summarize_lint_findings(findings: Vec<LintFinding>, active: &[&str]) -> LintSummary {
    policy::summarize_lint_findings(CATALOG, findings, active)
}

/// Gate conditions `finding_rules` violates, over the shipped rule set and the
/// run's `--deny`/`--warn` re-ranking.
#[must_use]
pub fn lint_gate_violations(
    overrides: &SeverityOverrides,
    options: LintPolicyOptions,
    finding_rules: &[&str],
) -> Vec<String> {
    policy::lint_gate_violations(CATALOG, overrides, options, finding_rules)
}

/// Judges a summary against the gate, over the shipped rule set and the run's
/// `--deny`/`--warn` re-ranking.
#[must_use]
pub fn evaluate_lint_policy(
    overrides: &SeverityOverrides,
    options: LintPolicyOptions,
    summary: &LintSummary,
) -> LintPolicy {
    policy::evaluate_lint_policy(CATALOG, overrides, options, summary)
}
