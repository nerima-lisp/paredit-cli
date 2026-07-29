use paredit_core_cli::CommandResult;

use crate::setf_arity::cli::args::SetfArityReportArgs;
use crate::setf_arity::cli::render::print_setf_arity_report;
use crate::setf_arity::usecase::{build_setf_arity_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn setf_arity_report(args: SetfArityReportArgs) -> CommandResult {
    // Explicit files only: this command has never expanded a directory
    // argument, and the envelope does not change what it accepts.
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_setf_arity_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_setf_arity_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "setf-arity-report policy failed: {message}"
        )));
    }

    Ok(())
}
