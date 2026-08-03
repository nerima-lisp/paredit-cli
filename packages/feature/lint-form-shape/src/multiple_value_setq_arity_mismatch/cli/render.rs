use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::multiple_value_setq_arity_mismatch::usecase::MultipleValueSetqArityMismatchItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_multiple_value_setq_arity_mismatch_report(
    reports: &[FileFindings<MultipleValueSetqArityMismatchItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect multiple-value-setq-arity-mismatch",
        reports,
        policy,
        output,
        verbosity,
    )
}
