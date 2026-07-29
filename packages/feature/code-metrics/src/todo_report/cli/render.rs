use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::todo_report::usecase::TaskMarker;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_marker_report(
    reports: &[FileFindings<TaskMarker>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect todo", reports, policy, output)
}
