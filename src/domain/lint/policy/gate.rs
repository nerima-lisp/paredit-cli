//! Summarizing a run and judging it against the caller's CI gate.

use std::collections::BTreeMap;

use crate::domain::lint::model::{LintFinding, LintPolicy, LintPolicyOptions, LintSummary};
use crate::domain::lint::registry::catalog::{RULES, rule_severity};

/// Summarizes findings, keeping only those from `active` rules; `per_rule`
/// lists exactly the active rules (in [`RULES`] order) so the checklist
/// reflects what actually ran.
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

/// Judges a summary against the requested gate, listing each violated
/// condition so the caller can report why the run failed.
pub fn evaluate_lint_policy(options: LintPolicyOptions, summary: &LintSummary) -> LintPolicy {
    let finding_count = summary.finding_count;
    let mut violations = Vec::new();
    if options.fail_on_finding() && finding_count > 0 {
        violations.push(format!("finding_count {finding_count} exceeds 0"));
    }
    if let Some(threshold) = options.fail_on_severity() {
        let count = summary
            .findings
            .iter()
            .filter(|finding| rule_severity(finding.rule).at_least(threshold))
            .count();
        if count > 0 {
            violations.push(format!(
                "{count} finding(s) at severity {} or higher",
                threshold.as_str()
            ));
        }
    }

    LintPolicy {
        fail_on_finding: options.fail_on_finding(),
        finding_count,
        passed: violations.is_empty(),
        violations,
    }
}
