use paredit_core_cli::CommandResult;

use crate::destructuring_bind_unused_whole::cli::args::DestructuringBindUnusedWholeReportArgs;
use crate::destructuring_bind_unused_whole::cli::render::print_destructuring_bind_unused_whole_report;
use crate::destructuring_bind_unused_whole::usecase::{
    build_destructuring_bind_unused_whole_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn destructuring_bind_unused_whole_report(
    args: DestructuringBindUnusedWholeReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_destructuring_bind_unused_whole_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_destructuring_bind_unused_whole_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "destructuring-bind-unused-whole-report policy failed: {message}"
        )));
    }

    Ok(())
}
