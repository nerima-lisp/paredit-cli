use paredit_core_cli::CommandResult;

use crate::loop_for_across_statically_known_list::cli::args::LoopForAcrossStaticallyKnownListReportArgs;
use crate::loop_for_across_statically_known_list::cli::render::print_loop_for_across_statically_known_list_report;
use crate::loop_for_across_statically_known_list::usecase::{
    build_loop_for_across_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn loop_for_across_statically_known_list_report(
    args: LoopForAcrossStaticallyKnownListReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_loop_for_across_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_loop_for_across_statically_known_list_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "loop-for-across-statically-known-list-report policy failed: {message}"
        )));
    }

    Ok(())
}
