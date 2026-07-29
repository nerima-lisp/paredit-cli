use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::zero_divisor::usecase::ZeroDivisorItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_zero_divisor_report(
    reports: &[FileFindings<ZeroDivisorItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect zero-divisor", reports, policy, output)
}
