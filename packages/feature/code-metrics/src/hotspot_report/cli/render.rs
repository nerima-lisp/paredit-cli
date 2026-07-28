use anyhow::Result;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::hotspot_report::usecase::Hotspot;

pub fn print_hotspot_report(
    reports: &[FileFindings<Hotspot>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect hotspots", reports, policy, output)
}
