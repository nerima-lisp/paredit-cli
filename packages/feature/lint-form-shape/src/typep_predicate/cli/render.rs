use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::typep_predicate::usecase::TypepPredicateItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_typep_predicate_report(
    reports: &[FileFindings<TypepPredicateItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect typep-predicate", reports, policy, output)
}
