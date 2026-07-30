use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::dead_boolean_operand::usecase::DeadBooleanOperandItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_dead_boolean_operand_report(
    reports: &[FileFindings<DeadBooleanOperandItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect dead-boolean-operand", reports, policy, output)
}
