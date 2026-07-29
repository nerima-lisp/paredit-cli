use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::make_hash_table_test::usecase::MakeHashTableTestItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_make_hash_table_test_report(
    reports: &[FileFindings<MakeHashTableTestItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect make-hash-table-test", reports, policy, output)
}
