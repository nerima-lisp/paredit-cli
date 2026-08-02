use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::atom_swap_with_side_effect::usecase::AtomSwapWithSideEffectItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_atom_swap_with_side_effect_report(
    reports: &[FileFindings<AtomSwapWithSideEffectItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect atom-swap-with-side-effect",
        reports,
        policy,
        output,
        verbosity,
    )
}
