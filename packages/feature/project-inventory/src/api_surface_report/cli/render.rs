use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::api_surface_report::usecase::ApiEntry;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_undefined_export_report(
    reports: &[FileFindings<ApiEntry>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect api-surface", reports, policy, output)
}
