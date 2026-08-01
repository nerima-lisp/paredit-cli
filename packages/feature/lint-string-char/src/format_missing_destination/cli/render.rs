use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::format_missing_destination::usecase::FormatMissingDestinationItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_format_missing_destination_report(
    reports: &[FileFindings<FormatMissingDestinationItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect format-missing-destination",
        reports,
        policy,
        output,
        verbosity,
    )
}
