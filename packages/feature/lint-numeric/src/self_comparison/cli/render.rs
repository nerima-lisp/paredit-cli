use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::self_comparison::usecase::SelfComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_self_comparison_report(
    reports: &[FileFindings<SelfComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect self-comparison", reports, policy, output)
}
