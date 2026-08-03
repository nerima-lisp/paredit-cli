use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::go_block_blocking_channel_op::usecase::GoBlockBlockingChannelOpItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_go_block_blocking_channel_op_report(
    reports: &[FileFindings<GoBlockBlockingChannelOpItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect go-block-blocking-channel-op",
        reports,
        policy,
        output,
        verbosity,
    )
}
