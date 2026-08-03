use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::let_star_independent_bindings::usecase::LetStarIndependentBindingsItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_let_star_independent_bindings_report(
    reports: &[FileFindings<LetStarIndependentBindingsItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect scheme-let-star-independent-bindings",
        reports,
        policy,
        output,
        verbosity,
    )
}
