use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::list_star_nil::usecase::ListStarNilItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_list_star_nil_report(
    reports: &[FileFindings<ListStarNilItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect list-star-nil", reports, policy, output)
}
