use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::runtime::Verbosity;

use crate::introspection_probe_unchecked::usecase::IntrospectionProbeUncheckedItem;

pub fn print_introspection_probe_unchecked_report(
    reports: &[FileFindings<IntrospectionProbeUncheckedItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect introspection-probe-unchecked",
        reports,
        policy,
        output,
        verbosity,
    )
}
