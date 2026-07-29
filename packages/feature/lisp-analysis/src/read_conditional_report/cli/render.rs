use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::read_conditional_report::usecase::ReadConditional;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_conditional_report(
    reports: &[FileFindings<ReadConditional>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect read-conditionals", reports, policy, output)
}
