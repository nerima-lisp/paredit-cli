use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::verbose_negation::usecase::VerboseNegationItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_verbose_negation_report(
    reports: &[FileFindings<VerboseNegationItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect verbose-negation", reports, policy, output)
}
