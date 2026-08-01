use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::lambda_list_keyword_order::usecase::LambdaListKeywordOrderItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_lambda_list_keyword_order_report(
    reports: &[FileFindings<LambdaListKeywordOrderItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect lambda-list-keyword-order",
        reports,
        policy,
        output,
        verbosity,
    )
}
