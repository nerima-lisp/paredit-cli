use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::nthcdr_small_index::usecase::NthcdrSmallIndexItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_nthcdr_small_index_report(
    reports: &[FileFindings<NthcdrSmallIndexItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect nthcdr-small-index",
        reports,
        policy,
        output,
        verbosity,
    )
}
