use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::unreachable_cond_clause::usecase::UnreachableCondClauseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unreachable_cond_clause_report(
    reports: &[FileFindings<UnreachableCondClauseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect unreachable-cond-clause", reports, policy, output)
}
