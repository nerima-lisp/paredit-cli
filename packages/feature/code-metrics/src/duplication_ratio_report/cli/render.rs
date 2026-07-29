use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::duplication_ratio_report::usecase::RepeatedShape;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplication_report(
    reports: &[FileFindings<RepeatedShape>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect duplication-ratio", reports, policy, output)
}
