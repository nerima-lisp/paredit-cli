use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::destructive_literal::usecase::DestructiveLiteralItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_destructive_literal_report(
    reports: &[FileFindings<DestructiveLiteralItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect destructive-literal", reports, policy, output)
}
