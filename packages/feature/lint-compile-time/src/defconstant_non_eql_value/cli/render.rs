use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::defconstant_non_eql_value::usecase::DefconstantNonEqlValueItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defconstant_non_eql_value_report(
    reports: &[FileFindings<DefconstantNonEqlValueItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect defconstant-non-eql-value",
        reports,
        policy,
        output,
        verbosity,
    )
}
