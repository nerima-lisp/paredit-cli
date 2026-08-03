use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::apply_with_literal_collection::usecase::ApplyWithLiteralCollectionItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_apply_with_literal_collection_report(
    reports: &[FileFindings<ApplyWithLiteralCollectionItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect apply-with-literal-collection",
        reports,
        policy,
        output,
        verbosity,
    )
}
