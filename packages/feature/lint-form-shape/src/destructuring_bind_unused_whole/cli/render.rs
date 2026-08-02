use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::destructuring_bind_unused_whole::usecase::DestructuringBindUnusedWholeItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_destructuring_bind_unused_whole_report(
    reports: &[FileFindings<DestructuringBindUnusedWholeItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect destructuring-bind-unused-whole",
        reports,
        policy,
        output,
        verbosity,
    )
}
