use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::redundant_start_zero::usecase::RedundantStartZeroItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_start_zero_report(
    reports: &[FileFindings<RedundantStartZeroItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect redundant-start-zero",
        reports,
        policy,
        output,
        verbosity,
    )
}
