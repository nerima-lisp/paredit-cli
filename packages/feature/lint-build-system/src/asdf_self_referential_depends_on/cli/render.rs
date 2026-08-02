use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::asdf_self_referential_depends_on::usecase::AsdfSelfReferentialDependsOnItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_asdf_self_referential_depends_on_report(
    reports: &[FileFindings<AsdfSelfReferentialDependsOnItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect asdf-self-referential-depends-on",
        reports,
        policy,
        output,
        verbosity,
    )
}
