use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::format_percent_ampersand_adjacent_redundancy::usecase::FormatPercentAmpersandAdjacentRedundancyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_format_percent_ampersand_adjacent_redundancy_report(
    reports: &[FileFindings<FormatPercentAmpersandAdjacentRedundancyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect format-percent-ampersand-adjacent-redundancy",
        reports,
        policy,
        output,
        verbosity,
    )
}
