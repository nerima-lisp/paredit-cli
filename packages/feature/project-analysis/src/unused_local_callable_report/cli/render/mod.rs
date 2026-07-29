use crate::unused_local_callable_report::usecase::{
    UnusedLocalCallablePolicy, UnusedLocalCallableReportFile,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;

mod json;
mod text;

pub fn print_unused_local_callable_report(
    reports: &[UnusedLocalCallableReportFile],
    policy: &UnusedLocalCallablePolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => text::print_unused_local_callable_report(reports, policy),
        OutputFormat::Json => json::print_unused_local_callable_report(reports, policy)?,
    }

    Ok(())
}
