use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::negated_comparison::usecase::NegatedComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_negated_comparison_report(
    reports: &[FileFindings<NegatedComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect negated-comparison",
        reports,
        policy,
        output,
        verbosity,
    )
}
