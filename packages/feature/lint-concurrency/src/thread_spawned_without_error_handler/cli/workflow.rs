use paredit_core_cli::{CliResult, CommandResult};

use crate::thread_spawned_without_error_handler::cli::args::ThreadSpawnedWithoutErrorHandlerReportArgs;
use crate::thread_spawned_without_error_handler::cli::render::print_thread_spawned_without_error_handler_report;
use crate::thread_spawned_without_error_handler::usecase::{
    build_thread_spawned_without_error_handler_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn thread_spawned_without_error_handler_report(
    args: ThreadSpawnedWithoutErrorHandlerReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_thread_spawned_without_error_handler_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_thread_spawned_without_error_handler_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "thread-spawned-without-error-handler-report policy failed: {message}"
        )));
    }

    Ok(())
}
