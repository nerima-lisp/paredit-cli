use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::make_array_default_keyword::usecase::MakeArrayDefaultKeywordItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_make_array_default_keyword_report(
    reports: &[FileFindings<MakeArrayDefaultKeywordItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect make-array-default-keyword",
        reports,
        policy,
        output,
        verbosity,
    )
}
