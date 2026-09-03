use paredit_core_cli::CommandResult;

use crate::around_method_missing_call_next_method::cli::args::AroundMethodMissingCallNextMethodReportArgs;
use crate::around_method_missing_call_next_method::cli::render::print_around_method_missing_call_next_method_report;
use crate::around_method_missing_call_next_method::usecase::{
    build_around_method_missing_call_next_method_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn around_method_missing_call_next_method_report(
    args: AroundMethodMissingCallNextMethodReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_around_method_missing_call_next_method_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_around_method_missing_call_next_method_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "around-method-missing-call-next-method-report policy failed: {message}"
        )));
    }

    Ok(())
}
