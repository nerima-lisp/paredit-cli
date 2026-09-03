use paredit_core_cli::CommandResult;

use crate::signal_on_error_condition_returns_silently::cli::args::SignalOnErrorConditionReturnsSilentlyReportArgs;
use crate::signal_on_error_condition_returns_silently::cli::render::print_signal_on_error_condition_returns_silently_report;
use crate::signal_on_error_condition_returns_silently::usecase::{
    build_signal_on_error_condition_returns_silently_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn signal_on_error_condition_returns_silently_report(
    args: SignalOnErrorConditionReturnsSilentlyReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_signal_on_error_condition_returns_silently_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_signal_on_error_condition_returns_silently_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "signal-on-error-condition-returns-silently-report policy failed: {message}"
        )));
    }

    Ok(())
}
