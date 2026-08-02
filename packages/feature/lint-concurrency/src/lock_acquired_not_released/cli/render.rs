use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::lock_acquired_not_released::usecase::LockAcquiredNotReleasedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_lock_acquired_not_released_report(
    reports: &[FileFindings<LockAcquiredNotReleasedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect lock-acquired-not-released",
        reports,
        policy,
        output,
        verbosity,
    )
}
