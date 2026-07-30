use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::nested_progn::usecase::NestedPrognItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nested_progn_report(
    reports: &[FileFindings<NestedPrognItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect nested-progn", reports, policy, output)
}
