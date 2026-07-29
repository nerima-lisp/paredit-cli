use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::identity_arithmetic::usecase::IdentityArithmeticItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_identity_arithmetic_report(
    reports: &[FileFindings<IdentityArithmeticItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect identity-arithmetic", reports, policy, output)
}
