use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::gethash_default::usecase::GethashDefaultItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_gethash_default_report(
    reports: &[FileFindings<GethashDefaultItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect gethash-default", reports, policy, output)
}
