use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::loop_into_accumulator_kind_conflict::usecase::LoopAccumulatorConflictItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_loop_into_accumulator_kind_conflict_report(
    reports: &[FileFindings<LoopAccumulatorConflictItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect loop-into-accumulator-kind-conflict",
        reports,
        policy,
        output,
        verbosity,
    )
}
