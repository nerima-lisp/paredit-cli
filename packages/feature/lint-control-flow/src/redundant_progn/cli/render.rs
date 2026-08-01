use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::redundant_progn::usecase::RedundantPrognItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_progn_report(
    reports: &[FileFindings<RedundantPrognItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect redundant-progn",
        reports,
        policy,
        output,
        verbosity,
    )
}
