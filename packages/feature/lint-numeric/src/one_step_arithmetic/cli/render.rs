use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::one_step_arithmetic::usecase::OneStepArithmeticItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_one_step_arithmetic_report(
    reports: &[FileFindings<OneStepArithmeticItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect one-step-arithmetic", reports, policy, output)
}
