use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::explicit_step_delta::usecase::ExplicitStepDeltaItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_explicit_step_delta_report(
    reports: &[FileFindings<ExplicitStepDeltaItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect explicit-step-delta", reports, policy, output)
}
