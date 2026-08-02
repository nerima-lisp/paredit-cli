use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::epsilon_less_float_loop_bound::usecase::EpsilonLessLoopItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_epsilon_less_float_loop_bound_report(
    reports: &[FileFindings<EpsilonLessLoopItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect epsilon-less-float-loop-bound",
        reports,
        policy,
        output,
        verbosity,
    )
}
