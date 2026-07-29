use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::format_newline::usecase::FormatNewlineItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_format_newline_report(
    reports: &[FileFindings<FormatNewlineItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect format-newline", reports, policy, output)
}
