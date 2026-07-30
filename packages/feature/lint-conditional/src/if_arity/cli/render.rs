use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::if_arity::usecase::IfArityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_if_arity_report(
    reports: &[FileFindings<IfArityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect if-arity", reports, policy, output)
}
