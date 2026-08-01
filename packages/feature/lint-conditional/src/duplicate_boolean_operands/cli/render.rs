use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_boolean_operands::usecase::DuplicateBooleanOperandItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_boolean_operand_report(
    reports: &[FileFindings<DuplicateBooleanOperandItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-boolean-operands",
        reports,
        policy,
        output,
        verbosity,
    )
}
