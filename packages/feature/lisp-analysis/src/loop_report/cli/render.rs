use anyhow::Result;

use paredit_core_cli::args::ReportFormat;

use crate::loop_report::usecase::LoopForm;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_unterminated_report(
    reports: &[FileFindings<LoopForm>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    print_report("inspect loop", reports, policy, output)
}
