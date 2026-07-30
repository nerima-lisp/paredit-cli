use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::negated_when_unless::usecase::NegatedWhenUnlessItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_negated_when_unless_report(
    reports: &[FileFindings<NegatedWhenUnlessItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect negated-when-unless", reports, policy, output)
}
