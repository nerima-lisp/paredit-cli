use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::single_clause_cond::usecase::SingleClauseCondItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_single_clause_cond_report(
    reports: &[FileFindings<SingleClauseCondItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect single-clause-cond", reports, policy, output)
}
