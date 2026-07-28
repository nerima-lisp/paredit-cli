use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::docstring_report::usecase::DocstringFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defect_report(
    reports: &[FileFindings<DocstringFinding>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect docstrings", reports, policy, output)
}
