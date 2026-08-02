use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::stringly_typed_dispatch::usecase::StringlyTypedDispatchItem;

pub fn print_stringly_typed_dispatch_report(
    reports: &[FileFindings<StringlyTypedDispatchItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect stringly-typed-dispatch",
        reports,
        policy,
        output,
        verbosity,
    )
}
