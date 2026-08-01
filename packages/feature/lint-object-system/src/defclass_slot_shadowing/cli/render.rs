use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::defclass_slot_shadowing::usecase::DefclassSlotShadowingItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defclass_slot_shadowing_report(
    reports: &[FileFindings<DefclassSlotShadowingItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect defclass-slot-shadowing",
        reports,
        policy,
        output,
        verbosity,
    )
}
