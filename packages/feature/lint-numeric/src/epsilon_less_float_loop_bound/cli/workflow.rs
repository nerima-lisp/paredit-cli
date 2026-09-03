use paredit_core_cli::CommandResult;

use crate::epsilon_less_float_loop_bound::cli::args::EpsilonLessFloatLoopBoundReportArgs;
use crate::epsilon_less_float_loop_bound::cli::render::print_epsilon_less_float_loop_bound_report;
use crate::epsilon_less_float_loop_bound::usecase::{
    build_epsilon_less_float_loop_bound_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn epsilon_less_float_loop_bound_report(
    args: EpsilonLessFloatLoopBoundReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_epsilon_less_float_loop_bound_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_epsilon_less_float_loop_bound_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "epsilon-less-float-loop-bound-report policy failed: {message}"
        )));
    }

    Ok(())
}
