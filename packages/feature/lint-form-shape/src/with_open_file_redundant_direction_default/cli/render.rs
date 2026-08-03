use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::with_open_file_redundant_direction_default::usecase::WithOpenFileRedundantDirectionDefaultItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_with_open_file_redundant_direction_default_report(
    reports: &[FileFindings<WithOpenFileRedundantDirectionDefaultItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect with-open-file-redundant-direction-default",
        reports,
        policy,
        output,
        verbosity,
    )
}
