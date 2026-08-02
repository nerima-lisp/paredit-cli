use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::thread_spawned_without_error_handler::usecase::ThreadSpawnedWithoutErrorHandlerItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_thread_spawned_without_error_handler_report(
    reports: &[FileFindings<ThreadSpawnedWithoutErrorHandlerItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect thread-spawned-without-error-handler",
        reports,
        policy,
        output,
        verbosity,
    )
}
