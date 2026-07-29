use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::macro_hygiene_report::usecase::HygieneFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_risk_report(
    reports: &[FileFindings<HygieneFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect macro-hygiene", reports, policy, output)
}
