use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_case_keys::usecase::DuplicateCaseKeyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_case_key_report(
    reports: &[FileFindings<DuplicateCaseKeyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-case-keys",
        reports,
        policy,
        output,
        verbosity,
    )
}
