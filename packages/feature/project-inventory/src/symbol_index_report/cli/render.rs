use anyhow::Result;

use paredit_core_cli::args::OutputFormat;

use crate::symbol_index_report::usecase::IndexEntry;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_external_report(
    reports: &[FileFindings<IndexEntry>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    print_report("inspect symbol-index", reports, policy, output)
}
