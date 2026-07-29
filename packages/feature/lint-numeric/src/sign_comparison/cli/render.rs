use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::sign_comparison::usecase::SignComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_sign_comparison_report(
    reports: &[FileFindings<SignComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect sign-comparison", reports, policy, output)
}
