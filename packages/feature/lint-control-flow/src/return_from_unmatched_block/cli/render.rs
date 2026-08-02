use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::return_from_unmatched_block::usecase::ReturnFromUnmatchedBlockItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_return_from_unmatched_block_report(
    reports: &[FileFindings<ReturnFromUnmatchedBlockItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect return-from-unmatched-block",
        reports,
        policy,
        output,
        verbosity,
    )
}
