use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::class_hierarchy_report::usecase::ClassFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_shadowed_slot_report(
    reports: &[FileFindings<ClassFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect class-hierarchy", reports, policy, output)
}
