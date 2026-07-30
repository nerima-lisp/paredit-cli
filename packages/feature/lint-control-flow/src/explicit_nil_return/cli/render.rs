use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::explicit_nil_return::usecase::ExplicitNilReturnItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_explicit_nil_return_report(
    reports: &[FileFindings<ExplicitNilReturnItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect explicit-nil-return", reports, policy, output)
}
