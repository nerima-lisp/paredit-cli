use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::when_unless_implicit_nil_misused::usecase::ImplicitNilItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_when_unless_implicit_nil_misused_report(
    reports: &[FileFindings<ImplicitNilItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect when-unless-implicit-nil-misused",
        reports,
        policy,
        output,
        verbosity,
    )
}
