use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::negated_step_delta::usecase::NegatedStepDeltaItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_negated_step_delta_report(
    reports: &[FileFindings<NegatedStepDeltaItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect negated-step-delta", reports, policy, output)
}
