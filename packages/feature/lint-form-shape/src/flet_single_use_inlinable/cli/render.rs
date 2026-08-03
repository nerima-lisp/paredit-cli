use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::flet_single_use_inlinable::usecase::FletSingleUseInlinableItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_flet_single_use_inlinable_report(
    reports: &[FileFindings<FletSingleUseInlinableItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect flet-single-use-inlinable",
        reports,
        policy,
        output,
        verbosity,
    )
}
