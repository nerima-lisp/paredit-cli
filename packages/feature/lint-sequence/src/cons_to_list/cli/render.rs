use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::cons_to_list::usecase::ConsToListItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_cons_to_list_report(
    reports: &[FileFindings<ConsToListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect cons-to-list", reports, policy, output)
}
