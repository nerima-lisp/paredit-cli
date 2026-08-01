use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplication_ratio_report::usecase::RepeatedShape;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplication_report(
    reports: &[FileFindings<RepeatedShape>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplication-ratio",
        reports,
        policy,
        output,
        verbosity,
    )
}
