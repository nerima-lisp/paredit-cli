use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::handler_case_no_clauses::usecase::HandlerCaseNoClausesItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_handler_case_no_clauses_report(
    reports: &[FileFindings<HandlerCaseNoClausesItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect handler-case-no-clauses",
        reports,
        policy,
        output,
        verbosity,
    )
}
