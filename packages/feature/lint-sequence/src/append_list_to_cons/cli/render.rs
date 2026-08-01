use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::append_list_to_cons::usecase::AppendListToConsItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_append_list_to_cons_report(
    reports: &[FileFindings<AppendListToConsItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect append-list-to-cons",
        reports,
        policy,
        output,
        verbosity,
    )
}
