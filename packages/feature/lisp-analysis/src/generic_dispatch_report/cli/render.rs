use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::generic_dispatch_report::usecase::GenericFinding;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defect_report(
    reports: &[FileFindings<GenericFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect generic-dispatch", reports, policy, output)
}
