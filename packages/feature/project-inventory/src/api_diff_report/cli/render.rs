use anyhow::Result;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::api_diff_report::usecase::ApiChange;

pub fn print_api_diff_report(
    reports: &[FileFindings<ApiChange>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect api-diff", reports, policy, output)
}
