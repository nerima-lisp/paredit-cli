use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_test_name::usecase::DuplicateTestNameItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_test_name_report(
    reports: &[FileFindings<DuplicateTestNameItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-test-name",
        reports,
        policy,
        output,
        verbosity,
    )
}
