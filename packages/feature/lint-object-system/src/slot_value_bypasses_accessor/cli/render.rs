use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::slot_value_bypasses_accessor::usecase::SlotValueBypassesAccessorItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_slot_value_bypasses_accessor_report(
    reports: &[FileFindings<SlotValueBypassesAccessorItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect slot-value-bypasses-accessor",
        reports,
        policy,
        output,
        verbosity,
    )
}
