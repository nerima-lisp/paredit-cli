use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::unsynchronized_shared_mutation::usecase::UnsynchronizedSharedMutationItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unsynchronized_shared_mutation_report(
    reports: &[FileFindings<UnsynchronizedSharedMutationItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect unsynchronized-shared-mutation",
        reports,
        policy,
        output,
        verbosity,
    )
}
