use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::parse_integer_default_radix::usecase::ParseIntegerDefaultRadixItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_parse_integer_default_radix_report(
    reports: &[FileFindings<ParseIntegerDefaultRadixItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report(
        "inspect parse-integer-default-radix",
        reports,
        policy,
        output,
    )
}
