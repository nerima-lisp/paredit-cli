use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::empty_let::usecase::EmptyLetItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_empty_let_report(
    reports: &[FileFindings<EmptyLetItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect empty-let", reports, policy, output)
}
