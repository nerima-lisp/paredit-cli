use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::package_lock_report::cli::args::PackageLockReportArgs;
use crate::package_lock_report::cli::render::print_undefined_behavior_report;
use crate::package_lock_report::usecase::{
    build_package_lock_report, evaluate_fail_on_undefined_behavior_policy,
};

pub fn package_lock_report(args: PackageLockReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_package_lock_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy =
        evaluate_fail_on_undefined_behavior_policy(args.fail_on_undefined_behavior, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_undefined_behavior_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect package-locks policy failed: {message}"
        )));
    }

    Ok(())
}
