use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_quote::usecase::RedundantQuoteItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_quote_report(
    reports: &[FileFindings<RedundantQuoteItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect redundant-quote", reports, policy, output)
}
