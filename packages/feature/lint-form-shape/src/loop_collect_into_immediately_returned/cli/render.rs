use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::loop_collect_into_immediately_returned::usecase::LoopCollectIntoImmediatelyReturnedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_loop_collect_into_immediately_returned_report(
    reports: &[FileFindings<LoopCollectIntoImmediatelyReturnedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect loop-collect-into-immediately-returned",
        reports,
        policy,
        output,
        verbosity,
    )
}
