use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::line_metrics_report::usecase::LineFinding;

pub fn print_line_metrics_report(
    reports: &[FileFindings<LineFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report("inspect line-metrics", reports, policy, output, verbosity)
}
