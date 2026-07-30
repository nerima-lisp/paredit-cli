use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::duplicate_keyword::usecase::DuplicateKeywordItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_keyword_report(
    reports: &[FileFindings<DuplicateKeywordItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect duplicate-keyword", reports, policy, output)
}
