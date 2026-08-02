use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::go_to_undefined_tag::usecase::GoToUndefinedTagItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_go_to_undefined_tag_report(
    reports: &[FileFindings<GoToUndefinedTagItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect go-to-undefined-tag",
        reports,
        policy,
        output,
        verbosity,
    )
}
