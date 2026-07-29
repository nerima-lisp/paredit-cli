use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_end_nil::usecase::RedundantEndNilItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_end_nil_report(
    reports: &[FileFindings<RedundantEndNilItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect redundant-end-nil", reports, policy, output)
}
