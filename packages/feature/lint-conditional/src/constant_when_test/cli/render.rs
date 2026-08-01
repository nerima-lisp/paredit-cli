use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::constant_when_test::usecase::ConstantWhenTestItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_constant_when_test_report(
    reports: &[FileFindings<ConstantWhenTestItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect constant-when-test",
        reports,
        policy,
        output,
        verbosity,
    )
}
