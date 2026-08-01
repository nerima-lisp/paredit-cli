use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::modify_macro_arity::usecase::ModifyMacroArityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_modify_macro_arity_report(
    reports: &[FileFindings<ModifyMacroArityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect modify-macro-arity",
        reports,
        policy,
        output,
        verbosity,
    )
}
