use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::test_map_report::usecase::CoverageEntry;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_untested_report(
    reports: &[FileFindings<CoverageEntry>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect test-map", reports, policy, output)
}
