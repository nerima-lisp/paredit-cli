use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::around_method_missing_call_next_method::usecase::AroundMethodMissingCallNextMethodItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_around_method_missing_call_next_method_report(
    reports: &[FileFindings<AroundMethodMissingCallNextMethodItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect around-method-missing-call-next-method",
        reports,
        policy,
        output,
        verbosity,
    )
}
