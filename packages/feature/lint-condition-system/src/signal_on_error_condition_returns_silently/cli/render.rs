use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::signal_on_error_condition_returns_silently::usecase::SignalOnErrorConditionReturnsSilentlyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_signal_on_error_condition_returns_silently_report(
    reports: &[FileFindings<SignalOnErrorConditionReturnsSilentlyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect signal-on-error-condition-returns-silently",
        reports,
        policy,
        output,
        verbosity,
    )
}
