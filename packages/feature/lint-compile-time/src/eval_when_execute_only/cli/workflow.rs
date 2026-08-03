use paredit_core_cli::CommandResult;

use crate::eval_when_execute_only::cli::args::EvalWhenExecuteOnlyReportArgs;
use crate::eval_when_execute_only::cli::render::print_eval_when_execute_only_report;
use crate::eval_when_execute_only::usecase::{
    build_eval_when_execute_only_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eval_when_execute_only_report(args: EvalWhenExecuteOnlyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_eval_when_execute_only_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_eval_when_execute_only_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eval-when-execute-only-report policy failed: {message}"
        )));
    }

    Ok(())
}
