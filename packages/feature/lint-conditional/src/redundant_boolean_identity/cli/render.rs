use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::redundant_boolean_identity::usecase::RedundantBooleanIdentityItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_boolean_identity_report(
    reports: &[FileFindings<RedundantBooleanIdentityItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report(
        "inspect redundant-boolean-identity",
        reports,
        policy,
        output,
    )
}
