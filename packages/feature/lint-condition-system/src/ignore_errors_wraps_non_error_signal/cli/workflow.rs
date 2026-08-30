use paredit_core_cli::{CliResult, CommandResult};

use crate::ignore_errors_wraps_non_error_signal::cli::args::IgnoreErrorsWrapsNonErrorSignalReportArgs;
use crate::ignore_errors_wraps_non_error_signal::cli::render::print_ignore_errors_wraps_non_error_signal_report;
use crate::ignore_errors_wraps_non_error_signal::usecase::{
    build_ignore_errors_wraps_non_error_signal_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn ignore_errors_wraps_non_error_signal_report(
    args: IgnoreErrorsWrapsNonErrorSignalReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_ignore_errors_wraps_non_error_signal_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_ignore_errors_wraps_non_error_signal_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "ignore-errors-wraps-non-error-signal-report policy failed: {message}"
        )));
    }

    Ok(())
}
