use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::generic_function_no_methods::usecase::GenericFunctionNoMethodsItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_generic_function_no_methods_report(
    reports: &[FileFindings<GenericFunctionNoMethodsItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect generic-function-no-methods",
        reports,
        policy,
        output,
        verbosity,
    )
}
