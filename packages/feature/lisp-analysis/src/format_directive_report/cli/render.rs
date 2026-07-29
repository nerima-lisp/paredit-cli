use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::format_directive_report::usecase::FormatCall;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_mismatch_report(
    reports: &[FileFindings<FormatCall>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect format-directives", reports, policy, output)
}
