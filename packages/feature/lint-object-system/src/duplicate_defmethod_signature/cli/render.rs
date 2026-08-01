use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

use crate::duplicate_defmethod_signature::usecase::DuplicateDefmethodSignatureItem;
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

pub fn print_duplicate_defmethod_signature_report(
    reports: &[FileFindings<DuplicateDefmethodSignatureItem>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    print_report(
        "inspect duplicate-defmethod-signature",
        reports,
        policy,
        output,
        verbosity,
    )
}
