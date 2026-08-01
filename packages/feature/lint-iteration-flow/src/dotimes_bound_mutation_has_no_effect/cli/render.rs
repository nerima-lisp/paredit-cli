use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::dotimes_bound_mutation_has_no_effect::usecase::DotimesBoundMutationItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_dotimes_bound_mutation_has_no_effect_report(
    reports: &[FileFindings<DotimesBoundMutationItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect dotimes-bound-mutation-has-no-effect",
        reports,
        policy,
        output,
        verbosity,
    )
}
