use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::subseq_zero::usecase::SubseqZeroItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_subseq_zero_report(
    reports: &[FileFindings<SubseqZeroItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect subseq-zero", reports, policy, output)
}
