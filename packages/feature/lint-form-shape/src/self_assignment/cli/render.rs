use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::self_assignment::usecase::SelfAssignmentItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_self_assignment_report(
    reports: &[FileFindings<SelfAssignmentItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect self-assignments",
        reports,
        policy,
        output,
        verbosity,
    )
}
