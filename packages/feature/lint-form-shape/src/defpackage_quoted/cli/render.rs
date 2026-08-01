use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::defpackage_quoted::usecase::DefpackageQuotedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defpackage_quoted_report(
    reports: &[FileFindings<DefpackageQuotedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect defpackage-quoted",
        reports,
        policy,
        output,
        verbosity,
    )
}
