use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::setf_arity::usecase::SetfArityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_setf_arity_report(
    reports: &[FileFindings<SetfArityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect setf-arity", reports, policy, output)
}
