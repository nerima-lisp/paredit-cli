use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::one_armed_if::usecase::OneArmedIfItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_one_armed_if_report(
    reports: &[FileFindings<OneArmedIfItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect one-armed-if", reports, policy, output)
}
