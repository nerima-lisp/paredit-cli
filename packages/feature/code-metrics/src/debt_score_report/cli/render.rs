use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::debt_score_report::usecase::DebtFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_debt_report(
    reports: &[FileFindings<DebtFinding>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect debt-score", reports, policy, output)
}
