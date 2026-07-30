use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::if_not::usecase::IfNotItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_if_not_report(
    reports: &[FileFindings<IfNotItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect if-not", reports, policy, output)
}
