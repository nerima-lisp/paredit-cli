use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::redundant_body_progn::usecase::RedundantBodyPrognItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_body_progn_report(
    reports: &[FileFindings<RedundantBodyPrognItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect redundant-body-progn",
        reports,
        policy,
        output,
        verbosity,
    )
}
