use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::string_case_fold::usecase::StringCaseFoldItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_string_case_fold_report(
    reports: &[FileFindings<StringCaseFoldItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect string-case-fold", reports, policy, output)
}
