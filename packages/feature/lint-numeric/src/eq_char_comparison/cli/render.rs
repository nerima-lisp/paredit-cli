use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::eq_char_comparison::usecase::EqCharComparisonItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_eq_char_comparison_report(
    reports: &[FileFindings<EqCharComparisonItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect eq-char-comparison", reports, policy, output)
}
