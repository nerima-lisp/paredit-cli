use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::api_diff_report::usecase::ApiChange;

pub fn print_api_diff_report(
    reports: &[FileFindings<ApiChange>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report("inspect api-diff", reports, policy, output, verbosity)
}
