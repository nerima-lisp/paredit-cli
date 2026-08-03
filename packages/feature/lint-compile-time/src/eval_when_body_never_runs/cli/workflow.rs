use paredit_core_cli::CommandResult;

use crate::eval_when_body_never_runs::cli::args::EvalWhenBodyNeverRunsReportArgs;
use crate::eval_when_body_never_runs::cli::render::print_eval_when_body_never_runs_report;
use crate::eval_when_body_never_runs::usecase::{
    build_eval_when_body_never_runs_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eval_when_body_never_runs_report(args: EvalWhenBodyNeverRunsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_eval_when_body_never_runs_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_eval_when_body_never_runs_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eval-when-body-never-runs-report policy failed: {message}"
        )));
    }

    Ok(())
}
