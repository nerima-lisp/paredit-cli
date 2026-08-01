use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::loop_clause_order_violation::usecase::LoopClauseOrderItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_loop_clause_order_violation_report(
    reports: &[FileFindings<LoopClauseOrderItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect loop-clause-order-violation",
        reports,
        policy,
        output,
        verbosity,
    )
}
