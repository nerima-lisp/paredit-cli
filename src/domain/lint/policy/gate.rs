//! Summarizing a run and judging it against the caller's CI gate.

use std::collections::BTreeMap;

use crate::domain::lint::model::{LintFinding, LintPolicy, LintPolicyOptions, LintSummary};
use crate::domain::lint::registry::catalog::{RULES, rule_severity};

/// Summarizes findings, keeping only those from `active` rules; `per_rule`
/// lists exactly the active rules (in [`RULES`] order) so the checklist
/// reflects what actually ran.
#[must_use]
pub fn summarize_lint_findings(findings: Vec<LintFinding>, active: &[&str]) -> LintSummary {
    let findings: Vec<LintFinding> = findings
        .into_iter()
        .filter(|finding| active.contains(&finding.rule))
        .collect();

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for finding in &findings {
        *counts.entry(finding.rule).or_insert(0) += 1;
    }
    let per_rule = RULES
        .iter()
        .copied()
        .filter(|rule| active.contains(rule))
        .map(|rule| (rule, counts.get(&rule).copied().unwrap_or(0)))
        .collect();

    LintSummary {
        finding_count: findings.len(),
        per_rule,
        findings,
    }
}

/// Every gate condition `finding_rules` violates, phrased for a report.
///
/// Takes the rule name of each reported finding rather than a whole summary,
/// because the SARIF and GitHub output paths have only that much: they stream
/// findings out as they are produced and never build one. Both readings go
/// through this function so the gate has a single implementation — two would
/// drift the moment a third condition is added.
#[must_use]
pub fn lint_gate_violations(options: LintPolicyOptions, finding_rules: &[&str]) -> Vec<String> {
    let mut violations = Vec::new();
    if options.fail_on_finding() && !finding_rules.is_empty() {
        violations.push(format!("finding_count {} exceeds 0", finding_rules.len()));
    }
    if let Some(threshold) = options.fail_on_severity() {
        let count = finding_rules
            .iter()
            .filter(|rule| rule_severity(rule).at_least(threshold))
            .count();
        if count > 0 {
            violations.push(format!(
                "{count} finding(s) at severity {} or higher",
                threshold.as_str()
            ));
        }
    }
    violations
}

/// Judges a summary against the requested gate, listing each violated
/// condition so the caller can report why the run failed.
#[must_use]
pub fn evaluate_lint_policy(options: LintPolicyOptions, summary: &LintSummary) -> LintPolicy {
    let finding_rules: Vec<&str> = summary
        .findings
        .iter()
        .map(|finding| finding.rule)
        .collect();
    let violations = lint_gate_violations(options, &finding_rules);

    LintPolicy {
        fail_on_finding: options.fail_on_finding(),
        finding_count: summary.finding_count,
        passed: violations.is_empty(),
        violations,
    }
}
