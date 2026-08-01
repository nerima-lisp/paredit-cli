use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::single_arg_comparison::usecase::SingleArgComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_single_arg_comparison_report(
    reports: &[FileFindings<SingleArgComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect single-arg-comparison",
        reports,
        policy,
        output,
        verbosity,
    )
}
