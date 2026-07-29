use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::double_reverse::usecase::DoubleReverseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_double_reverse_report(
    reports: &[FileFindings<DoubleReverseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect double-reverse", reports, policy, output)
}
