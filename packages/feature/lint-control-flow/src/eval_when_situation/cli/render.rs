use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::eval_when_situation::usecase::EvalWhenSituationItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eval_when_situation_report(
    reports: &[FileFindings<EvalWhenSituationItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect eval-when-situation", reports, policy, output)
}
