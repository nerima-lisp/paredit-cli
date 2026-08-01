use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::define_condition_missing_report_for_error_type::usecase::DefineConditionMissingReportForErrorTypeItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_define_condition_missing_report_for_error_type_report(
    reports: &[FileFindings<DefineConditionMissingReportForErrorTypeItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect define-condition-missing-report-for-error-type",
        reports,
        policy,
        output,
        verbosity,
    )
}
