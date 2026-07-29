use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::if_to_unless::usecase::IfToUnlessItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_if_to_unless_report(
    reports: &[FileFindings<IfToUnlessItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect if-to-unless", reports, policy, output)
}
