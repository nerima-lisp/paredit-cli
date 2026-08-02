use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::test_without_assertion::usecase::TestWithoutAssertionItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_test_without_assertion_report(
    reports: &[FileFindings<TestWithoutAssertionItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect test-without-assertion",
        reports,
        policy,
        output,
        verbosity,
    )
}
