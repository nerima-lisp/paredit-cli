use serde_json::json;

use crate::unused_parameter_report::usecase::{
    UnusedParameterReportFile, UnusedParameterReportPolicy,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;

mod json;
mod text;

pub fn print_unused_parameter_report(
    reports: &[UnusedParameterReportFile],
    policy: &UnusedParameterReportPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => text::print_unused_parameter_report(reports, policy),
        OutputFormat::Json => json::print_unused_parameter_report(reports, policy)?,
    }

    Ok(())
}
