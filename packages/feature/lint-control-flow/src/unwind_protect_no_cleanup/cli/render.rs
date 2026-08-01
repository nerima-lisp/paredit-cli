use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::unwind_protect_no_cleanup::usecase::UnwindProtectNoCleanupItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unwind_protect_no_cleanup_report(
    reports: &[FileFindings<UnwindProtectNoCleanupItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect unwind-protect-no-cleanup",
        reports,
        policy,
        output,
        verbosity,
    )
}
