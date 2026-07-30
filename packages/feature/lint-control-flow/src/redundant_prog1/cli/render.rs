use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_prog1::usecase::RedundantProg1Item;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_prog1_report(
    reports: &[FileFindings<RedundantProg1Item>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect redundant-prog1", reports, policy, output)
}
