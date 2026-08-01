use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::multiple_value_list_of_values::usecase::MultipleValueListOfValuesItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_multiple_value_list_of_values_report(
    reports: &[FileFindings<MultipleValueListOfValuesItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect multiple-value-list-of-values",
        reports,
        policy,
        output,
        verbosity,
    )
}
