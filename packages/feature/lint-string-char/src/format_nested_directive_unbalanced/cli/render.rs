use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::format_nested_directive_unbalanced::usecase::FormatNestedDirectiveUnbalancedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_format_nested_directive_unbalanced_report(
    reports: &[FileFindings<FormatNestedDirectiveUnbalancedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect format-nested-directive-unbalanced",
        reports,
        policy,
        output,
        verbosity,
    )
}
