use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::ftype_values_arity_mismatch::usecase::FtypeValuesArityMismatchItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_ftype_values_arity_mismatch_report(
    reports: &[FileFindings<FtypeValuesArityMismatchItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect ftype-values-arity-mismatch",
        reports,
        policy,
        output,
        verbosity,
    )
}
