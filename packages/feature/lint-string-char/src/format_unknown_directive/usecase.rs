//! Unknown-format-directive (a ~ directive CLHS 22.3 does not define) detection.

pub use crate::format_unknown_directive::domain::{
    FormatUnknownDirectiveItem, build_format_unknown_directive_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An unknown directive is a
/// run-time failure in every implementation the author is not using, but
/// CLHS does not reserve the unlisted characters, so a project that
/// deliberately uses an implementation extension is entitled to say so by
/// leaving the gate off.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatUnknownDirectiveItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} format control string(s) with a directive CLHS does not define",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
