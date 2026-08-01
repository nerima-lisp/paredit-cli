use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::quoted_case_key::usecase::QuotedCaseKeyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_quoted_case_key_report(
    reports: &[FileFindings<QuotedCaseKeyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect quoted-case-key",
        reports,
        policy,
        output,
        verbosity,
    )
}
