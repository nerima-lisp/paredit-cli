use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::eval_when_body_never_runs::usecase::EvalWhenBodyNeverRunsItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eval_when_body_never_runs_report(
    reports: &[FileFindings<EvalWhenBodyNeverRunsItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect eval-when-body-never-runs",
        reports,
        policy,
        output,
        verbosity,
    )
}
