use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::malformed_case_clause::usecase::MalformedCaseClauseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_malformed_case_clause_report(
    reports: &[FileFindings<MalformedCaseClauseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect malformed-case-clause", reports, policy, output)
}
