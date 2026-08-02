use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::case_key_eql_pitfall::usecase::CaseKeyEqlPitfallItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_case_key_eql_pitfall_report(
    reports: &[FileFindings<CaseKeyEqlPitfallItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect case-key-eql-pitfall",
        reports,
        policy,
        output,
        verbosity,
    )
}
