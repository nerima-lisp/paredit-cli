use paredit_core_cli::CommandResult;

use crate::ignore_errors_wraps_non_error_signal::cli::args::IgnoreErrorsWrapsNonErrorSignalReportArgs;
use crate::ignore_errors_wraps_non_error_signal::cli::render::print_ignore_errors_wraps_non_error_signal_report;
use crate::ignore_errors_wraps_non_error_signal::usecase::{
    build_ignore_errors_wraps_non_error_signal_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn ignore_errors_wraps_non_error_signal_report(
    args: IgnoreErrorsWrapsNonErrorSignalReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_ignore_errors_wraps_non_error_signal_report(
            file, dialect, &tree,
        )?);
    }

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
