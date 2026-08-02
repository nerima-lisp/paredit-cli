use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::quoted_form_contains_stray_unquote::usecase::QuotedFormContainsStrayUnquoteItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_quoted_form_contains_stray_unquote_report(
    reports: &[FileFindings<QuotedFormContainsStrayUnquoteItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect quoted-form-contains-stray-unquote",
        reports,
        policy,
        output,
        verbosity,
    )
}
