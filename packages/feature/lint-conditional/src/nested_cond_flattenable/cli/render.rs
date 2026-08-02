use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::nested_cond_flattenable::usecase::NestedCondItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nested_cond_flattenable_report(
    reports: &[FileFindings<NestedCondItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect nested-cond-flattenable",
        reports,
        policy,
        output,
        verbosity,
    )
}
