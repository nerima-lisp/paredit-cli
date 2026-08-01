use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::unreachable_case_clause::usecase::UnreachableCaseClauseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unreachable_case_clause_report(
    reports: &[FileFindings<UnreachableCaseClauseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect unreachable-case-clause",
        reports,
        policy,
        output,
        verbosity,
    )
}
