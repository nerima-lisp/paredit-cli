use serde_json::json;

use crate::shadowed_binding_report::usecase::{ShadowedBindingPolicy, ShadowedBindingReportFile};
use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;

mod json;
mod text;

pub fn print_shadowed_binding_report(
    reports: &[ShadowedBindingReportFile],
    policy: &ShadowedBindingPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => text::print_shadowed_binding_report(reports, policy),
        OutputFormat::Json => json::print_shadowed_binding_report(reports, policy)?,
    }

    Ok(())
}
