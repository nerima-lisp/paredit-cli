use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::ignore_errors_wraps_non_error_signal::usecase::IgnoreErrorsWrapsNonErrorSignalItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_ignore_errors_wraps_non_error_signal_report(
    reports: &[FileFindings<IgnoreErrorsWrapsNonErrorSignalItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect ignore-errors-wraps-non-error-signal",
        reports,
        policy,
        output,
        verbosity,
    )
}
