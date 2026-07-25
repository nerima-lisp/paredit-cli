use super::super::*;
use crate::application::usecase::shadowed_binding_report::{
    ShadowedBindingPolicy, ShadowedBindingReportFile,
};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_shadowed_binding_report(
    reports: &[ShadowedBindingReportFile],
    policy: &ShadowedBindingPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_shadowed_binding_report(reports, policy),
        OutputFormat::Json => json::print_shadowed_binding_report(reports, policy)?,
    }

    Ok(())
}
