use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::deeply_nested_anonymous_lambda::usecase::DeeplyNestedLambdaItem;

pub fn print_deeply_nested_anonymous_lambda_report(
    reports: &[FileFindings<DeeplyNestedLambdaItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect deeply-nested-anonymous-lambda",
        reports,
        policy,
        output,
        verbosity,
    )
}
