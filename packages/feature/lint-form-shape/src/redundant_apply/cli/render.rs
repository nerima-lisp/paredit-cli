use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_apply::usecase::RedundantApplyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_apply_report(
    reports: &[FileFindings<RedundantApplyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect redundant-apply", reports, policy, output)
}
