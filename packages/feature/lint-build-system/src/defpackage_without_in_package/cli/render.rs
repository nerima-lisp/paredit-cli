use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::defpackage_without_in_package::usecase::DefpackageWithoutInPackageItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defpackage_without_in_package_report(
    reports: &[FileFindings<DefpackageWithoutInPackageItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect defpackage-without-in-package",
        reports,
        policy,
        output,
        verbosity,
    )
}
