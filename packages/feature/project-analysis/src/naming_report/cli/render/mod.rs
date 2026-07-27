use super::super::*;
use crate::application::usecase::naming_report::{NamingReportFile, NamingReportPolicy};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_naming_report(
    reports: &[NamingReportFile],
    policy: &NamingReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_naming_report(reports, policy),
        OutputFormat::Json => json::print_naming_report(reports, policy)?,
    }

    Ok(())
}
