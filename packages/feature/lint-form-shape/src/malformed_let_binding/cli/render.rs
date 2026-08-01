use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::malformed_let_binding::usecase::MalformedLetBindingItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_malformed_let_binding_report(
    reports: &[FileFindings<MalformedLetBindingItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect malformed-let-binding",
        reports,
        policy,
        output,
        verbosity,
    )
}
