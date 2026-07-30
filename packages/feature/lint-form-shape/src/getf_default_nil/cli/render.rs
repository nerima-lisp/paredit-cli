use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::getf_default_nil::usecase::GetfDefaultNilItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_getf_default_nil_report(
    reports: &[FileFindings<GetfDefaultNilItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect getf-default-nil", reports, policy, output)
}
