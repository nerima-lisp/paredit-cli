use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::memq_assq_literal_key::usecase::MemqAssqLiteralKeyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_memq_assq_literal_key_report(
    reports: &[FileFindings<MemqAssqLiteralKeyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect scheme-memq-assq-literal-key",
        reports,
        policy,
        output,
        verbosity,
    )
}
