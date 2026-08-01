use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::method_qualifier_typo::usecase::MethodQualifierTypoItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_method_qualifier_typo_report(
    reports: &[FileFindings<MethodQualifierTypoItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect method-qualifier-typo",
        reports,
        policy,
        output,
        verbosity,
    )
}
