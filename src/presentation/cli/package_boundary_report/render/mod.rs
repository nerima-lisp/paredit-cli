use super::super::*;
use crate::application::usecase::package_boundary_report::{
    PackageBoundaryPolicy, PackageBoundaryReportFile,
};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_package_boundary_report(
    reports: &[PackageBoundaryReportFile],
    policy: &PackageBoundaryPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_package_boundary_report(reports, policy),
        OutputFormat::Json => json::print_package_boundary_report(reports, policy)?,
    }

    Ok(())
}
