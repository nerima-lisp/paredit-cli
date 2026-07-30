use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::case_nil_key::usecase::CaseNilKeyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_case_nil_key_report(
    reports: &[FileFindings<CaseNilKeyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect case-nil-key", reports, policy, output)
}
