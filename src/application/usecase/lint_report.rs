//! Aggregate syntactic-lint pass across explicit files.

pub use crate::domain::lint_report::{
    CATEGORIES, FIXABLE_RULES, LintFinding, LintPolicy, LintPolicyOptions, LintSummary, RULE_DOCS,
    RULES, RuleFixFor, Severity, collect_lint_findings, collect_lint_findings_and_fixes,
    collect_lint_fixes as collect_lint_fixes_for, evaluate_lint_policy, lint_gate_violations,
    resolve_active_rules, rule_category, rule_description, rule_is_fixable, rule_severity,
    summarize_lint_findings,
};
pub use crate::domain::lint_suppression::{LintSuppressions, UnusedSuppression};
