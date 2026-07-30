use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::de_morgan::usecase::DeMorganItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_de_morgan_report(
    reports: &[FileFindings<DeMorganItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect de-morgan", reports, policy, output)
}
