use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::malformed_cond_clause::usecase::MalformedCondClauseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_malformed_cond_clause_report(
    reports: &[FileFindings<MalformedCondClauseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect malformed-cond-clause",
        reports,
        policy,
        output,
        verbosity,
    )
}
