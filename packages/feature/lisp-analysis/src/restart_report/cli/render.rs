use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::restart_report::usecase::RestartFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unpaired_report(
    reports: &[FileFindings<RestartFinding>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect restarts", reports, policy, output)
}
