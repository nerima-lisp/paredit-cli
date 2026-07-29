use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::method_combination_report::usecase::MethodFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_orphaned_report(
    reports: &[FileFindings<MethodFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect method-combination", reports, policy, output)
}
