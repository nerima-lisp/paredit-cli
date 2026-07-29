use super::super::*;
use crate::application::usecase::unused_parameter_report::{
    UnusedParameterReportFile, UnusedParameterReportPolicy,
};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_unused_parameter_report(
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
