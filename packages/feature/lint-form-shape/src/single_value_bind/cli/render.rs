use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::single_value_bind::usecase::SingleValueBindItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_single_value_bind_report(
    reports: &[FileFindings<SingleValueBindItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect single-value-bind", reports, policy, output)
}
