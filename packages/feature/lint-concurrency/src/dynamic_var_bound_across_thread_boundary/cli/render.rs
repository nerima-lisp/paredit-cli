use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::dynamic_var_bound_across_thread_boundary::usecase::DynamicVarBoundAcrossThreadBoundaryItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_dynamic_var_bound_across_thread_boundary_report(
    reports: &[FileFindings<DynamicVarBoundAcrossThreadBoundaryItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect dynamic-var-bound-across-thread-boundary",
        reports,
        policy,
        output,
        verbosity,
    )
}
