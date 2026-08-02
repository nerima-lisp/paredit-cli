use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::return_outside_implicit_nil_block::usecase::ReturnOutsideImplicitNilBlockItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_return_outside_implicit_nil_block_report(
    reports: &[FileFindings<ReturnOutsideImplicitNilBlockItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect return-outside-implicit-nil-block",
        reports,
        policy,
        output,
        verbosity,
    )
}
