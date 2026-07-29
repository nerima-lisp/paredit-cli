use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::nth_constant_index::usecase::NthConstantIndexItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nth_constant_index_report(
    reports: &[FileFindings<NthConstantIndexItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect nth-constant-index", reports, policy, output)
}
