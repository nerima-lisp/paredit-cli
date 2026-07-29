use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::nested_when::usecase::NestedWhenItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nested_when_report(
    reports: &[FileFindings<NestedWhenItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect nested-when", reports, policy, output)
}
