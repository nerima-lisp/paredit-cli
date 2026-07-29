use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::serial_consistency_report::usecase::ComponentFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_fault_report(
    reports: &[FileFindings<ComponentFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect serial-consistency", reports, policy, output)
}
