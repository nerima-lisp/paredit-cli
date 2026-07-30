use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::append_nil::usecase::AppendNilItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_append_nil_report(
    reports: &[FileFindings<AppendNilItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect append-nil", reports, policy, output)
}
