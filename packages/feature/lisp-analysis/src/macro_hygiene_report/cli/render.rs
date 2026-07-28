use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::macro_hygiene_report::usecase::HygieneFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_risk_report(
    reports: &[FileFindings<HygieneFinding>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect macro-hygiene", reports, policy, output)
}
