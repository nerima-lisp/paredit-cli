use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::define_condition_empty_superclass_list::usecase::DefineConditionEmptySuperclassListItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_define_condition_empty_superclass_list_report(
    reports: &[FileFindings<DefineConditionEmptySuperclassListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect define-condition-empty-superclass-list",
        reports,
        policy,
        output,
        verbosity,
    )
}
