use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::exhaustive_case_otherwise::usecase::ExhaustiveCaseOtherwiseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_exhaustive_case_otherwise_report(
    reports: &[FileFindings<ExhaustiveCaseOtherwiseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect exhaustive-case-otherwise", reports, policy, output)
}
