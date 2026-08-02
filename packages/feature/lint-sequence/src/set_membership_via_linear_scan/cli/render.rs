use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::set_membership_via_linear_scan::usecase::LinearScanItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_set_membership_via_linear_scan_report(
    reports: &[FileFindings<LinearScanItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect set-membership-via-linear-scan",
        reports,
        policy,
        output,
        verbosity,
    )
}
