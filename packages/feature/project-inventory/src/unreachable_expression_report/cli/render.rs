use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::unreachable_expression_report::usecase::UnreachableExpression;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unreachable_report(
    reports: &[FileFindings<UnreachableExpression>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect unreachable-expressions", reports, policy, output)
}
