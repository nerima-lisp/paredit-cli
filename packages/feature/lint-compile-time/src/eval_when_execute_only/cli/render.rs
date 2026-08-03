use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::eval_when_execute_only::usecase::EvalWhenExecuteOnlyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eval_when_execute_only_report(
    reports: &[FileFindings<EvalWhenExecuteOnlyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect eval-when-execute-only",
        reports,
        policy,
        output,
        verbosity,
    )
}
