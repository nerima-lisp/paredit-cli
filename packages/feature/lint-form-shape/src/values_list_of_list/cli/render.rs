use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::values_list_of_list::usecase::ValuesListOfListItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_values_list_of_list_report(
    reports: &[FileFindings<ValuesListOfListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect values-list-of-list",
        reports,
        policy,
        output,
        verbosity,
    )
}
