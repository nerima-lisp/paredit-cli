use crate::package_boundary_report::usecase::{PackageBoundaryPolicy, PackageBoundaryReportFile};
use anyhow::Result;
use paredit_core_cli::args::OutputFormat;

mod json;
mod text;

pub fn print_package_boundary_report(
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
