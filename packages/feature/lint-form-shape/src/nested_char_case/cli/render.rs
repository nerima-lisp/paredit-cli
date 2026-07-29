use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::nested_char_case::usecase::NestedCharCaseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nested_char_case_report(
    reports: &[FileFindings<NestedCharCaseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect nested-char-case", reports, policy, output)
}
