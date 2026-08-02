use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::asdf_perform_without_call_next_method::usecase::AsdfPerformWithoutCallNextMethodItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_asdf_perform_without_call_next_method_report(
    reports: &[FileFindings<AsdfPerformWithoutCallNextMethodItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect asdf-perform-without-call-next-method",
        reports,
        policy,
        output,
        verbosity,
    )
}
