use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::cond_to_case_candidate::usecase::CondToCaseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_cond_to_case_candidate_report(
    reports: &[FileFindings<CondToCaseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect cond-to-case-candidate",
        reports,
        policy,
        output,
        verbosity,
    )
}
