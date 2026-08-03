use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::hash_table_iteration_order_assumed::usecase::HashOrderItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_hash_table_iteration_order_assumed_report(
    reports: &[FileFindings<HashOrderItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect hash-table-iteration-order-assumed",
        reports,
        policy,
        output,
        verbosity,
    )
}
