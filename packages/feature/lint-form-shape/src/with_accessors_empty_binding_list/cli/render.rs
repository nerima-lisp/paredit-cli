use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::with_accessors_empty_binding_list::usecase::WithAccessorsEmptyBindingListItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_with_accessors_empty_binding_list_report(
    reports: &[FileFindings<WithAccessorsEmptyBindingListItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect with-accessors-empty-binding-list",
        reports,
        policy,
        output,
        verbosity,
    )
}
