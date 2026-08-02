use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::mixed_float_precision_arithmetic::usecase::MixedFloatPrecisionItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_mixed_float_precision_arithmetic_report(
    reports: &[FileFindings<MixedFloatPrecisionItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect mixed-float-precision-arithmetic",
        reports,
        policy,
        output,
        verbosity,
    )
}
