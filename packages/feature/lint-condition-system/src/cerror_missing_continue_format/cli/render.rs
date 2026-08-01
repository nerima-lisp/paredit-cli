use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::cerror_missing_continue_format::usecase::CerrorMissingContinueFormatItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_cerror_missing_continue_format_report(
    reports: &[FileFindings<CerrorMissingContinueFormatItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect cerror-missing-continue-format",
        reports,
        policy,
        output,
        verbosity,
    )
}
