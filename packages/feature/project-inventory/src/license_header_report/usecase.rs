//! inspect license-headers reporting across a set of files.

pub use crate::license_header_report::domain::{
    HeaderStatus, LicenseHeaderItem, build_license_header_report,
};

use std::collections::BTreeMap;

use paredit_core_cli::report::{FileFindings, ReportPolicy};
use serde_json::json;

/// Re-examines a fileset's headers once every file has been analyzed on its
/// own, flagging a [`HeaderStatus::Present`] header whose text differs from
/// the majority header as [`HeaderStatus::Inconsistent`].
///
/// This cannot happen inside [`build_license_header_report`]: whether one
/// file's header is the odd one out is a question about the whole fileset,
/// and no single file's analysis can see the others. It runs once, after
/// every file has been read, and mutates each report's findings and summary
/// in place rather than rebuilding them, since [`FileFindings::new`] would
/// need the source text back to re-sort a set that is already in order.
///
/// Ties are broken by the header text sorting last, which is arbitrary but
/// deterministic: a fileset with two headers tied for most common has no
/// principled majority to pick, and a stable answer beats a random one.
pub fn apply_header_consistency(reports: &mut [FileFindings<LicenseHeaderItem>]) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for report in reports.iter() {
        for finding in &report.findings {
            if finding.status == HeaderStatus::Present {
                if let Some(text) = finding.header_text.as_deref() {
                    *counts.entry(text).or_default() += 1;
                }
            }
        }
    }

    let Some(majority) = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(text, _)| text.to_owned())
    else {
        return;
    };

    for report in reports.iter_mut() {
        let mut inconsistent = 0usize;
        for finding in &mut report.findings {
            if finding.status != HeaderStatus::Present {
                continue;
            }
            if finding.header_text.as_deref() == Some(majority.as_str()) {
                continue;
            }
            finding.status = HeaderStatus::Inconsistent;
            inconsistent += 1;
        }
        if let Some(entry) = report
            .summary
            .iter_mut()
            .find(|(name, _)| *name == "inconsistent_count")
        {
            entry.1 = json!(inconsistent);
        }
    }
}

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only a missing header can fail a build. A
/// header present but inconsistent is a review-worthy finding, not
/// something `--fail-on-missing-header` promises to catch.
#[must_use]
pub fn evaluate_fail_on_missing_header_policy(
    fail_on_missing_header: bool,
    reports: &[FileFindings<LicenseHeaderItem>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|entry| entry.status == HeaderStatus::Missing))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_missing_header.then_some("--fail-on-missing-header"),
        &failing,
        |report| format!("{} is missing a license header", report.path.display()),
    );
    // The headline count stays the number of entries reported; only the
    // gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report(name: &str, source: &str) -> FileFindings<LicenseHeaderItem> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_license_header_report(Path::new(name), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_header_matching_the_majority_stays_present() {
        let mut reports = vec![
            report("a.lisp", ";; Copyright 2024\n(defun a () 1)"),
            report("b.lisp", ";; Copyright 2024\n(defun b () 2)"),
        ];
        apply_header_consistency(&mut reports);
        assert!(
            reports
                .iter()
                .all(|report| report.findings[0].status == HeaderStatus::Present)
        );
        assert_eq!(reports[0].summary[1], ("inconsistent_count", json!(0)));
    }

    #[test]
    fn a_header_differing_from_the_majority_is_flagged_inconsistent() {
        let mut reports = vec![
            report("a.lisp", ";; Copyright 2024\n(defun a () 1)"),
            report("b.lisp", ";; Copyright 2024\n(defun b () 2)"),
            report("c.lisp", ";; Copyright 2019 Stale Corp\n(defun c () 3)"),
        ];
        apply_header_consistency(&mut reports);
        assert_eq!(reports[0].findings[0].status, HeaderStatus::Present);
        assert_eq!(reports[1].findings[0].status, HeaderStatus::Present);
        assert_eq!(reports[2].findings[0].status, HeaderStatus::Inconsistent);
        assert_eq!(reports[2].summary[1], ("inconsistent_count", json!(1)));
    }

    #[test]
    fn a_missing_header_is_never_reclassified_as_inconsistent() {
        let mut reports = vec![
            report("a.lisp", ";; Copyright 2024\n(defun a () 1)"),
            report("b.lisp", "(defun b () 2)"),
        ];
        apply_header_consistency(&mut reports);
        assert_eq!(reports[1].findings[0].status, HeaderStatus::Missing);
    }

    #[test]
    fn a_fileset_with_no_present_header_leaves_every_file_untouched() {
        let mut reports = vec![report("a.lisp", "(defun a () 1)")];
        apply_header_consistency(&mut reports);
        assert_eq!(reports[0].findings[0].status, HeaderStatus::Missing);
    }

    #[test]
    fn the_gate_fails_only_on_missing_not_on_inconsistent() {
        let mut reports = vec![
            report("a.lisp", ";; Copyright 2024\n(defun a () 1)"),
            report("b.lisp", ";; Copyright 2024\n(defun b () 2)"),
            report("c.lisp", ";; Copyright 2019 Stale Corp\n(defun c () 3)"),
        ];
        apply_header_consistency(&mut reports);
        let policy = evaluate_fail_on_missing_header_policy(true, &reports);
        assert!(policy.passed);
        assert!(policy.violations.is_empty());
    }

    #[test]
    fn the_gate_fails_when_a_file_is_missing_a_header() {
        let reports = vec![report("a.lisp", "(defun a () 1)")];
        let policy = evaluate_fail_on_missing_header_policy(true, &reports);
        assert!(!policy.passed);
        assert_eq!(policy.violations.len(), 1);
    }

    #[test]
    fn the_gate_is_disarmed_by_default() {
        let reports = vec![report("a.lisp", "(defun a () 1)")];
        let policy = evaluate_fail_on_missing_header_policy(false, &reports);
        assert!(policy.passed);
    }
}
