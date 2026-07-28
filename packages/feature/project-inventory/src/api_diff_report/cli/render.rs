use anyhow::Result;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::api_diff_report::usecase::ApiChange;

pub fn print_api_diff_report(
    reports: &[FileFindings<ApiChange>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect api-diff", reports, policy, output)
}
