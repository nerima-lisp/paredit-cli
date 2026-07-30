use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::duplicate_parameters::usecase::DuplicateParameterItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_parameter_report(
    reports: &[FileFindings<DuplicateParameterItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect duplicate-parameters", reports, policy, output)
}
