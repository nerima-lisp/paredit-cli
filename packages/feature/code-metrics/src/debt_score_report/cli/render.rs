use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::debt_score_report::usecase::DebtFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_debt_report(
    reports: &[FileFindings<DebtFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect debt-score", reports, policy, output)
}
