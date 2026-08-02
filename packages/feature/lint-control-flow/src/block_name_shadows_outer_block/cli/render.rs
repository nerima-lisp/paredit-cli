use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::block_name_shadows_outer_block::usecase::BlockNameShadowsOuterBlockItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_block_name_shadows_outer_block_report(
    reports: &[FileFindings<BlockNameShadowsOuterBlockItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect block-name-shadows-outer-block",
        reports,
        policy,
        output,
        verbosity,
    )
}
