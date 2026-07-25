//! Aggregate syntactic-lint pass across explicit files.

pub use crate::domain::lint_report::{
    CATEGORIES, FIXABLE_RULES, LintFinding, LintPolicy, LintPolicyOptions, LintSummary, RULE_DOCS,
    RULES, Severity, collect_lint_findings, evaluate_lint_policy, resolve_active_rules,
    rule_category, rule_description, rule_is_fixable, rule_severity, summarize_lint_findings,
};
pub use crate::domain::lint_suppression::{LintSuppressions, UnusedSuppression};
