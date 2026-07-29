use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_funcall::usecase::RedundantFuncallItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_funcall_report(
    reports: &[FileFindings<RedundantFuncallItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect redundant-funcall", reports, policy, output)
}
