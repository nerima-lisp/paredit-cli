use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::def_inside_function_body::usecase::DefInsideFunctionBodyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_def_inside_function_body_report(
    reports: &[FileFindings<DefInsideFunctionBodyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect def-inside-function-body",
        reports,
        policy,
        output,
        verbosity,
    )
}
