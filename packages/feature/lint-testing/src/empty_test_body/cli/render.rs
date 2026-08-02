use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::empty_test_body::usecase::EmptyTestBodyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_empty_test_body_report(
    reports: &[FileFindings<EmptyTestBodyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect empty-test-body",
        reports,
        policy,
        output,
        verbosity,
    )
}
