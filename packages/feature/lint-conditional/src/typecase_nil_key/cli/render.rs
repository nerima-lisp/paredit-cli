use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::typecase_nil_key::usecase::TypecaseNilKeyItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_typecase_nil_key_report(
    reports: &[FileFindings<TypecaseNilKeyItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect typecase-nil-key", reports, policy, output)
}
