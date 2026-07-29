use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::sharp_quoted_lambda::usecase::SharpQuotedLambdaItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_sharp_quoted_lambda_report(
    reports: &[FileFindings<SharpQuotedLambdaItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect sharp-quoted-lambda", reports, policy, output)
}
