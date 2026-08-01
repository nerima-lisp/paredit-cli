use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::single_operand_arithmetic::usecase::SingleOperandArithmeticItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_single_operand_arithmetic_report(
    reports: &[FileFindings<SingleOperandArithmeticItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect single-operand-arithmetic",
        reports,
        policy,
        output,
        verbosity,
    )
}
