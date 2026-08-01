use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::loop_for_across_statically_known_list::usecase::LoopForAcrossListItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_loop_for_across_statically_known_list_report(
    reports: &[FileFindings<LoopForAcrossListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect loop-for-across-statically-known-list",
        reports,
        policy,
        output,
        verbosity,
    )
}
