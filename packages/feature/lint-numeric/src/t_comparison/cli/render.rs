use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::t_comparison::usecase::TComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_t_comparison_report(
    reports: &[FileFindings<TComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect t-comparison", reports, policy, output)
}
