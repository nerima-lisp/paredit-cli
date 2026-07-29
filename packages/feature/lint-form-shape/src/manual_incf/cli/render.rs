use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::manual_incf::usecase::ManualIncfItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_manual_incf_report(
    reports: &[FileFindings<ManualIncfItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect manual-incf", reports, policy, output)
}
