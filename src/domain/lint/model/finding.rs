//! What a lint run produces: findings, and the summary a report renders.

use std::path::PathBuf;

use crate::domain::sexpr::ByteSpan;

use super::RuleFix;

/// One rule's complaint about one form.
///
/// The field shapes are the published contract of `inspect lint` (text, JSON,
/// and SARIF all project directly from them) and are deliberately primitive:
/// `rule` stays a `&'static str` so consumers outside the domain need no
/// domain types to read a report.
#[derive(Debug, Clone)]
pub struct LintFinding {
    pub rule: &'static str,
    pub path: PathBuf,
    pub span: ByteSpan,
    pub message: String,
}

/// A finding together with the rewrite its rule can apply, if any.
///
/// Findings and fixes are produced by the same visit rather than by two passes,
/// because a fix generally needs data the finding does not carry (the inner
/// span to splice, the complement operator to substitute). Re-deriving that
/// from a `LintFinding` alone is impossible; re-walking the tree to recover it
/// is what the old presentation-layer fix collector did.
#[derive(Debug, Clone)]
pub struct LintOutcome {
    finding: LintFinding,
    fix: Option<RuleFix>,
}

impl LintOutcome {
    pub const fn new(finding: LintFinding, fix: Option<RuleFix>) -> Self {
        Self { finding, fix }
    }

    pub fn into_parts(self) -> (LintFinding, Option<RuleFix>) {
        (self.finding, self.fix)
    }
}

/// The findings of one run, filtered to the active rules, plus the per-rule
/// checklist a report prints.
#[derive(Debug)]
pub struct LintSummary {
    pub finding_count: usize,
    /// Count of findings per rule, in `RULES` order (rules with zero findings
    /// are included so a consumer sees the full checklist).
    pub per_rule: Vec<(&'static str, usize)>,
    pub findings: Vec<LintFinding>,
}
