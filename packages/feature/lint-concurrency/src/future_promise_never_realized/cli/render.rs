use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::future_promise_never_realized::usecase::FuturePromiseNeverRealizedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_future_promise_never_realized_report(
    reports: &[FileFindings<FuturePromiseNeverRealizedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect future-promise-never-realized",
        reports,
        policy,
        output,
        verbosity,
    )
}
