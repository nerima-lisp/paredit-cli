use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::make_list_default_element::usecase::MakeListDefaultElementItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_make_list_default_element_report(
    reports: &[FileFindings<MakeListDefaultElementItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect make-list-default-element", reports, policy, output)
}
