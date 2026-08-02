use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::overly_long_parameter_list::usecase::LongParameterListItem;

pub fn print_overly_long_parameter_list_report(
    reports: &[FileFindings<LongParameterListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect overly-long-parameter-list",
        reports,
        policy,
        output,
        verbosity,
    )
}
