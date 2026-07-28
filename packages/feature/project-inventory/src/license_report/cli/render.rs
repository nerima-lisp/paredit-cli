use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::license_report::usecase::SystemLicense;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_review_report(
    reports: &[FileFindings<SystemLicense>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect licenses", reports, policy, output)
}
