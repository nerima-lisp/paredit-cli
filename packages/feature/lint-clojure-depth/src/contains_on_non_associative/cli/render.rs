use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::contains_on_non_associative::usecase::ContainsOnNonAssociativeItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_contains_on_non_associative_report(
    reports: &[FileFindings<ContainsOnNonAssociativeItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect contains-on-non-associative",
        reports,
        policy,
        output,
        verbosity,
    )
}
