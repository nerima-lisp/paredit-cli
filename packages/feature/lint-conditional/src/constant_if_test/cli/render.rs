use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::constant_if_test::usecase::ConstantIfTestItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_constant_if_test_report(
    reports: &[FileFindings<ConstantIfTestItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect constant-if-test",
        reports,
        policy,
        output,
        verbosity,
    )
}
