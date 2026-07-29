use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::negated_if::usecase::NegatedIfItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_negated_if_report(
    reports: &[FileFindings<NegatedIfItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect negated-if", reports, policy, output)
}
