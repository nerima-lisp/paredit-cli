use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::equality_arity::usecase::EqualityArityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_equality_arity_report(
    reports: &[FileFindings<EqualityArityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect equality-arity", reports, policy, output)
}
