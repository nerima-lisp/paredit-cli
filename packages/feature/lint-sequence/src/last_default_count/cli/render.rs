use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::last_default_count::usecase::LastDefaultCountItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_last_default_count_report(
    reports: &[FileFindings<LastDefaultCountItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect last-default-count",
        reports,
        policy,
        output,
        verbosity,
    )
}
