use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::eql_string_comparison::usecase::EqlStringComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eql_string_comparison_report(
    reports: &[FileFindings<EqlStringComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect eql-string-comparison", reports, policy, output)
}
