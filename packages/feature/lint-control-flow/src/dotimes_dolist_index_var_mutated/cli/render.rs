use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::dotimes_dolist_index_var_mutated::usecase::DotimesDolistIndexVarMutatedItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_dotimes_dolist_index_var_mutated_report(
    reports: &[FileFindings<DotimesDolistIndexVarMutatedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect dotimes-dolist-index-var-mutated",
        reports,
        policy,
        output,
        verbosity,
    )
}
