use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::nil_comparison::usecase::NilComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nil_comparison_report(
    reports: &[FileFindings<NilComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report("inspect nil-comparison", reports, policy, output, verbosity)
}
