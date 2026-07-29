use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::circular_literal_report::usecase::CircularLiteral;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_label_report(
    reports: &[FileFindings<CircularLiteral>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect circular-literals", reports, policy, output)
}
