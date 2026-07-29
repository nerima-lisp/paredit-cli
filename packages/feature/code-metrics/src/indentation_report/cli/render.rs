use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::indentation_report::usecase::IndentFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_deviation_report(
    reports: &[FileFindings<IndentFinding>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect indentation", reports, policy, output)
}
