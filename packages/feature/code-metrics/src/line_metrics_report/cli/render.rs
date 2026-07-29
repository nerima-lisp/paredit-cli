use anyhow::Result;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::line_metrics_report::usecase::LineFinding;

pub fn print_line_metrics_report(
    reports: &[FileFindings<LineFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect line-metrics", reports, policy, output)
}
