use paredit_core_cli::CommandResult;

use crate::loop_unreachable_finally_clause::cli::args::LoopUnreachableFinallyClauseReportArgs;
use crate::loop_unreachable_finally_clause::cli::render::print_loop_unreachable_finally_clause_report;
use crate::loop_unreachable_finally_clause::usecase::{
    build_loop_unreachable_finally_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn loop_unreachable_finally_clause_report(
    args: LoopUnreachableFinallyClauseReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_loop_unreachable_finally_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_loop_unreachable_finally_clause_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "loop-unreachable-finally-clause-report policy failed: {message}"
        )));
    }

    Ok(())
}
