use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::literal_place::usecase::LiteralPlaceItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_literal_place_report(
    reports: &[FileFindings<LiteralPlaceItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect literal-place", reports, policy, output)
}
