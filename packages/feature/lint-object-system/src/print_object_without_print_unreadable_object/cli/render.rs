use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::print_object_without_print_unreadable_object::usecase::PrintObjectWithoutPrintUnreadableObjectItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_print_object_without_print_unreadable_object_report(
    reports: &[FileFindings<PrintObjectWithoutPrintUnreadableObjectItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect print-object-without-print-unreadable-object",
        reports,
        policy,
        output,
        verbosity,
    )
}
