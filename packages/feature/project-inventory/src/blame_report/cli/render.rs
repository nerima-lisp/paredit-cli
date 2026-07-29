use anyhow::Result;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::blame_report::usecase::Attribution;

pub fn print_blame_report(
    reports: &[FileFindings<Attribution>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect blame", reports, policy, output)
}
