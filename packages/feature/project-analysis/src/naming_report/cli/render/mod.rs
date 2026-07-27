use crate::naming_report::usecase::{NamingReportFile, NamingReportPolicy};
use anyhow::Result;
use paredit_core_cli::args::OutputFormat;

mod json;
mod text;

pub fn print_naming_report(
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
