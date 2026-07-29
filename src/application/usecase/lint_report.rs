//! Aggregate syntactic-lint pass across explicit files.

pub use crate::domain::lint_report::{
    CATEGORIES, EXPERIMENTAL_RULES, FIXABLE_RULES, FindingId, LintFinding, LintPassRequest,
    LintPassResult, LintPolicy, LintPolicyOptions, LintSummary, PEDANTIC_RULES, RULE_DOCS, RULES,
    RuleExample, RuleExplanation, RuleFilter, RuleFixFor, RulePreset, RuleSetting, RuleSettings,
    RuleTag, RuleTags, RuleTimings, Severity, SeverityOverrides, TAGS, WARNING_RULES,
    apply_severity_override, collect_lint_findings, collect_lint_findings_and_fixes,
    collect_lint_fixes as collect_lint_fixes_for, evaluate_lint_policy, lint_gate_violations,
    overridden_rule_severity, resolve_active_rules, rule_category, rule_description, rule_dialects,
    rule_explanation, rule_is_fixable, rule_setting, rule_settings, rule_severity, rule_tags,
    rule_timing_report, run_lint_pass, summarize_lint_findings,
};
pub use crate::domain::lint_suppression::{
    Date, LintSuppressions, SuppressionInventoryEntry, UnusedSuppression,
};
