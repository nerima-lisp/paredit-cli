use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::handler_bind_handler_returns_bare_value::usecase::HandlerBindHandlerReturnsBareValueItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_handler_bind_handler_returns_bare_value_report(
    reports: &[FileFindings<HandlerBindHandlerReturnsBareValueItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect handler-bind-handler-returns-bare-value",
        reports,
        policy,
        output,
        verbosity,
    )
}
