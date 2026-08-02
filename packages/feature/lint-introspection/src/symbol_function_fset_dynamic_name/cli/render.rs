use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::symbol_function_fset_dynamic_name::usecase::SymbolFunctionFsetDynamicNameItem;

pub fn print_symbol_function_fset_dynamic_name_report(
    reports: &[FileFindings<SymbolFunctionFsetDynamicNameItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect symbol-function-fset-dynamic-name",
        reports,
        policy,
        output,
        verbosity,
    )
}
