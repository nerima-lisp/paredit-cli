use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::package_lock_report::usecase::PackageLock;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_undefined_behavior_report(
    reports: &[FileFindings<PackageLock>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect package-locks", reports, policy, output)
}
