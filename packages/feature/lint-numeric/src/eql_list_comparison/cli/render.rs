use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::eql_list_comparison::usecase::EqlListComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eql_list_comparison_report(
    reports: &[FileFindings<EqlListComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect eql-list-comparison", reports, policy, output)
}
