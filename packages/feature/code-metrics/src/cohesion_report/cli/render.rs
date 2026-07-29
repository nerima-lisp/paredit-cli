use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::cohesion_report::usecase::DefinitionCoupling;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_isolated_report(
    reports: &[FileFindings<DefinitionCoupling>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect cohesion", reports, policy, output)
}
