use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::division_result_precision_loss::usecase::DivisionPrecisionLossItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_division_result_precision_loss_report(
    reports: &[FileFindings<DivisionPrecisionLossItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect division-result-precision-loss",
        reports,
        policy,
        output,
        verbosity,
    )
}
