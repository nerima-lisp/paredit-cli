use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::accessor_arity::usecase::AccessorArityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_accessor_arity_report(
    reports: &[FileFindings<AccessorArityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect accessor-arity", reports, policy, output)
}
