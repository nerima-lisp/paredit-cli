use paredit_core_cli::{CliResult, CommandResult};

use crate::setf_arity::cli::args::SetfArityReportArgs;
use crate::setf_arity::cli::render::print_setf_arity_report;
use crate::setf_arity::usecase::{build_setf_arity_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn setf_arity_report(args: SetfArityReportArgs) -> CommandResult {
    // Explicit files only: this command has never expanded a directory
    // argument, and the envelope does not change what it accepts.
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_setf_arity_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_setf_arity_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "setf-arity-report policy failed: {message}"
        )));
    }

    Ok(())
}
