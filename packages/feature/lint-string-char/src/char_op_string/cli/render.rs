use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::char_op_string::usecase::CharOpStringItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_char_op_string_report(
    reports: &[FileFindings<CharOpStringItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect char-op-string", reports, policy, output)
}
