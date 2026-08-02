use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::tagbody_unreachable_tag::usecase::TagbodyUnreachableTagItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_tagbody_unreachable_tag_report(
    reports: &[FileFindings<TagbodyUnreachableTagItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect tagbody-unreachable-tag",
        reports,
        policy,
        output,
        verbosity,
    )
}
