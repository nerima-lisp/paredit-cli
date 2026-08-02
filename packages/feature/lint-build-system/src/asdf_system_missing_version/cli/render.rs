use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::asdf_system_missing_version::usecase::AsdfSystemMissingVersionItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_asdf_system_missing_version_report(
    reports: &[FileFindings<AsdfSystemMissingVersionItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect asdf-system-missing-version",
        reports,
        policy,
        output,
        verbosity,
    )
}
