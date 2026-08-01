use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::prog2_to_progn::usecase::Prog2ToPrognItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_prog2_to_progn_report(
    reports: &[FileFindings<Prog2ToPrognItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report("inspect prog2-to-progn", reports, policy, output, verbosity)
}
