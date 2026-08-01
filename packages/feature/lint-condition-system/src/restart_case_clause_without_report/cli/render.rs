use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::restart_case_clause_without_report::usecase::RestartCaseClauseWithoutReportItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_restart_case_clause_without_report_report(
    reports: &[FileFindings<RestartCaseClauseWithoutReportItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect restart-case-clause-without-report",
        reports,
        policy,
        output,
        verbosity,
    )
}
