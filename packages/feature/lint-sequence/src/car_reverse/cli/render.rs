use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::car_reverse::usecase::CarReverseItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_car_reverse_report(
    reports: &[FileFindings<CarReverseItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect car-reverse", reports, policy, output)
}
