use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::recursive_lock_reentry_risk::usecase::RecursiveLockReentryRiskItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_recursive_lock_reentry_risk_report(
    reports: &[FileFindings<RecursiveLockReentryRiskItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect recursive-lock-reentry-risk",
        reports,
        policy,
        output,
        verbosity,
    )
}
