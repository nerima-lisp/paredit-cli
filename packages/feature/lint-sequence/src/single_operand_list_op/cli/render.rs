use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::single_operand_list_op::usecase::SingleOperandListOpItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_single_operand_list_op_report(
    reports: &[FileFindings<SingleOperandListOpItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect single-operand-list-op", reports, policy, output)
}
