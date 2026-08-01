use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::butlast_default_count::usecase::ButlastDefaultCountItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_butlast_default_count_report(
    reports: &[FileFindings<ButlastDefaultCountItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect butlast-default-count",
        reports,
        policy,
        output,
        verbosity,
    )
}
