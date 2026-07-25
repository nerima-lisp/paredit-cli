use super::super::*;
use crate::application::usecase::unused_local_callable_report::{
    UnusedLocalCallablePolicy, UnusedLocalCallableReportFile,
};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_unused_local_callable_report(
    reports: &[UnusedLocalCallableReportFile],
    policy: &UnusedLocalCallablePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_unused_local_callable_report(reports, policy),
        OutputFormat::Json => json::print_unused_local_callable_report(reports, policy)?,
    }

    Ok(())
}
