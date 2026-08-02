use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::nested_function_parameter_shadows_enclosing_parameter::usecase::NestedParameterShadowItem;

pub fn print_nested_parameter_shadow_report(
    reports: &[FileFindings<NestedParameterShadowItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect nested-function-parameter-shadows-enclosing-parameter",
        reports,
        policy,
        output,
        verbosity,
    )
}
