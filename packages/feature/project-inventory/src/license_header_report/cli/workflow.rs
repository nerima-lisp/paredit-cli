use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::license_header_report::cli::args::LicenseHeaderReportArgs;
use crate::license_header_report::cli::render::print_license_header_report;
use crate::license_header_report::usecase::{
    apply_header_consistency, build_license_header_report, evaluate_fail_on_missing_header_policy,
};

pub fn license_header_report(args: LicenseHeaderReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_license_header_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let mut reports = analysis.succeeded;

    apply_header_consistency(&mut reports);

    let policy = evaluate_fail_on_missing_header_policy(args.fail_on_missing_header, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_license_header_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect license-headers policy failed: {message}"
        )));
    }

    Ok(())
}
