use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::identical_if_branches::usecase::IdenticalIfBranchItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_identical_if_branch_report(
    reports: &[FileFindings<IdenticalIfBranchItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect identical-if-branches",
        reports,
        policy,
        output,
        verbosity,
    )
}
