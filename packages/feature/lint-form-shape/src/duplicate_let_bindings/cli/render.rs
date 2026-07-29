use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::duplicate_let_bindings::usecase::DuplicateLetBindingItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_let_binding_report(
    reports: &[FileFindings<DuplicateLetBindingItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect duplicate-let-bindings", reports, policy, output)
}
