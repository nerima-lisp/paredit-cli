use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::keyword_arity_report::usecase::ArityFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_fault_report(
    reports: &[FileFindings<ArityFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect keyword-arity", reports, policy, output)
}
