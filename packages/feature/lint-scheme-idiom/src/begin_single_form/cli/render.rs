use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::begin_single_form::usecase::BeginSingleFormItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_begin_single_form_report(
    reports: &[FileFindings<BeginSingleFormItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect scheme-begin-single-form",
        reports,
        policy,
        output,
        verbosity,
    )
}
