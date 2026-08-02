use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::intern_dynamic_package_target::usecase::InternDynamicPackageTargetItem;

pub fn print_intern_dynamic_package_target_report(
    reports: &[FileFindings<InternDynamicPackageTargetItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect intern-dynamic-package-target",
        reports,
        policy,
        output,
        verbosity,
    )
}
