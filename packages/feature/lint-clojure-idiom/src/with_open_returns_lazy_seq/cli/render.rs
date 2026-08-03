use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::with_open_returns_lazy_seq::usecase::WithOpenLazySeqItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_with_open_returns_lazy_seq_report(
    reports: &[FileFindings<WithOpenLazySeqItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect with-open-returns-lazy-seq",
        reports,
        policy,
        output,
        verbosity,
    )
}
