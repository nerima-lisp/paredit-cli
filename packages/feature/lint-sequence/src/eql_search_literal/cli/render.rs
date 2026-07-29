use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::eql_search_literal::usecase::EqlSearchLiteralItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eql_search_literal_report(
    reports: &[FileFindings<EqlSearchLiteralItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect eql-search-literal", reports, policy, output)
}
