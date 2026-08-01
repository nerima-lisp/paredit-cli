use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::list_star_to_cons::usecase::ListStarToConsItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_list_star_to_cons_report(
    reports: &[FileFindings<ListStarToConsItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect list-star-to-cons",
        reports,
        policy,
        output,
        verbosity,
    )
}
