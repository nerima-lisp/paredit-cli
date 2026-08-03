use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::format_unknown_directive::usecase::FormatUnknownDirectiveItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_format_unknown_directive_report(
    reports: &[FileFindings<FormatUnknownDirectiveItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect format-unknown-directive",
        reports,
        policy,
        output,
        verbosity,
    )
}
