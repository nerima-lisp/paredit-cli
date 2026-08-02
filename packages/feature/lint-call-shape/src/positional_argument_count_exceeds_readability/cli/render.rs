use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::positional_argument_count_exceeds_readability::usecase::PositionalLiteralCallItem;

pub fn print_positional_argument_count_report(
    reports: &[FileFindings<PositionalLiteralCallItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect positional-argument-count-exceeds-readability",
        reports,
        policy,
        output,
        verbosity,
    )
}
