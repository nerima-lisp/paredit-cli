use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

use crate::find_report::usecase::PatternHit;

pub fn print_find_report(
    reports: &[FileFindings<PatternHit>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("query find", reports, policy, output)
}
