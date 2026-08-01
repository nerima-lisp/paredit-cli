use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::dolist_result_form_references_loop_variable::usecase::DolistResultVariableItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_dolist_result_form_references_loop_variable_report(
    reports: &[FileFindings<DolistResultVariableItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect dolist-result-form-references-loop-variable",
        reports,
        policy,
        output,
        verbosity,
    )
}
