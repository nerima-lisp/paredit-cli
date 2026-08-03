use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::redundant_precision_coercion::usecase::RedundantPrecisionCoercionItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_redundant_precision_coercion_report(
    reports: &[FileFindings<RedundantPrecisionCoercionItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect redundant-precision-coercion",
        reports,
        policy,
        output,
        verbosity,
    )
}
