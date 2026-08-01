use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_lambda_list_keyword::usecase::DuplicateLambdaListKeywordItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_lambda_list_keyword_report(
    reports: &[FileFindings<DuplicateLambdaListKeywordItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-lambda-list-keyword",
        reports,
        policy,
        output,
        verbosity,
    )
}
