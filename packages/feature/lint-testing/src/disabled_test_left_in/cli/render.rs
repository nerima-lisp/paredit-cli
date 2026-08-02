use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::disabled_test_left_in::usecase::DisabledTestLeftInItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_disabled_test_left_in_report(
    reports: &[FileFindings<DisabledTestLeftInItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect disabled-test-left-in",
        reports,
        policy,
        output,
        verbosity,
    )
}
