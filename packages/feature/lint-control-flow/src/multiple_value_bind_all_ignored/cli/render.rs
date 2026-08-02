use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::multiple_value_bind_all_ignored::usecase::MultipleValueBindAllIgnoredItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_multiple_value_bind_all_ignored_report(
    reports: &[FileFindings<MultipleValueBindAllIgnoredItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect multiple-value-bind-all-ignored",
        reports,
        policy,
        output,
        verbosity,
    )
}
