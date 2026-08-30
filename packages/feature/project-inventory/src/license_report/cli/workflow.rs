use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::license_report::cli::args::LicenseReportArgs;
use crate::license_report::cli::render::print_review_report;
use crate::license_report::usecase::{build_license_report, evaluate_fail_on_review_policy};

pub fn license_report(args: LicenseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_license_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_review_policy(args.fail_on_review, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_review_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect licenses policy failed: {message}"
        )));
    }

    Ok(())
}
