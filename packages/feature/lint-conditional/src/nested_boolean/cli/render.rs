use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::nested_boolean::usecase::NestedBooleanItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nested_boolean_report(
    reports: &[FileFindings<NestedBooleanItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect nested-boolean", reports, policy, output)
}
