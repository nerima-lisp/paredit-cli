use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::hotspot_report::usecase::Hotspot;

pub fn print_hotspot_report(
    reports: &[FileFindings<Hotspot>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect hotspots", reports, policy, output)
}
