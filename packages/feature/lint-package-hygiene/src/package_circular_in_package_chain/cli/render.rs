use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::package_circular_in_package_chain::usecase::CircularInPackageChainItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_circular_in_package_chain_report(
    reports: &[FileFindings<CircularInPackageChainItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect package-circular-in-package-chain",
        reports,
        policy,
        output,
        verbosity,
    )
}
