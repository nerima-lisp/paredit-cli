use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::named_let_never_recurs::usecase::NamedLetNeverRecursItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_named_let_never_recurs_report(
    reports: &[FileFindings<NamedLetNeverRecursItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect scheme-named-let-never-recurs",
        reports,
        policy,
        output,
        verbosity,
    )
}
