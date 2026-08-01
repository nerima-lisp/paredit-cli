use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_cond_tests::usecase::DuplicateCondTestItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_cond_test_report(
    reports: &[FileFindings<DuplicateCondTestItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-cond-tests",
        reports,
        policy,
        output,
        verbosity,
    )
}
