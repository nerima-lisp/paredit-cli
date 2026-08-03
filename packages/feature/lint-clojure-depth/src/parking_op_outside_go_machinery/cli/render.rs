use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::parking_op_outside_go_machinery::usecase::ParkingOpOutsideGoMachineryItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_parking_op_outside_go_machinery_report(
    reports: &[FileFindings<ParkingOpOutsideGoMachineryItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect parking-op-outside-go-machinery",
        reports,
        policy,
        output,
        verbosity,
    )
}
