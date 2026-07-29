use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::read_time_eval_report::usecase::ReadTimeEval;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_read_eval_report(
    reports: &[FileFindings<ReadTimeEval>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect read-time-eval", reports, policy, output)
}
