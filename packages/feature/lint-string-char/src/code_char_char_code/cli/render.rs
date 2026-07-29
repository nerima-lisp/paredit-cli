use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::code_char_char_code::usecase::CodeCharCharCodeItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_code_char_char_code_report(
    reports: &[FileFindings<CodeCharCharCodeItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect code-char-char-code", reports, policy, output)
}
