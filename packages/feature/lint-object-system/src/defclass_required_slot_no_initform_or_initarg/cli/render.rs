use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::defclass_required_slot_no_initform_or_initarg::usecase::DefclassRequiredSlotNoInitformOrInitargItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_defclass_required_slot_no_initform_or_initarg_report(
    reports: &[FileFindings<DefclassRequiredSlotNoInitformOrInitargItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect defclass-required-slot-no-initform-or-initarg",
        reports,
        policy,
        output,
        verbosity,
    )
}
