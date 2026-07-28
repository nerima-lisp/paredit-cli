use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::readtable_case_report::usecase::CaseSensitiveSymbol;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_fragile_report(
    reports: &[FileFindings<CaseSensitiveSymbol>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect readtable-case", reports, policy, output)
}
