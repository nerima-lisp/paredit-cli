use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::test_asserts_constant::usecase::TestAssertsConstantItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_test_asserts_constant_report(
    reports: &[FileFindings<TestAssertsConstantItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect test-asserts-constant",
        reports,
        policy,
        output,
        verbosity,
    )
}
