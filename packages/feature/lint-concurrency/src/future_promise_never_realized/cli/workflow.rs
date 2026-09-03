use paredit_core_cli::CommandResult;

use crate::future_promise_never_realized::cli::args::FuturePromiseNeverRealizedReportArgs;
use crate::future_promise_never_realized::cli::render::print_future_promise_never_realized_report;
use crate::future_promise_never_realized::usecase::{
    build_future_promise_never_realized_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn future_promise_never_realized_report(
    args: FuturePromiseNeverRealizedReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_future_promise_never_realized_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_future_promise_never_realized_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "future-promise-never-realized-report policy failed: {message}"
        )));
    }

    Ok(())
}
