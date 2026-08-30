use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::todo_report::cli::args::TodoReportArgs;
use crate::todo_report::cli::render::print_marker_report;
use crate::todo_report::usecase::{build_todo_report, evaluate_fail_on_marker_policy};

pub fn todo_report(args: TodoReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_todo_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_marker_policy(args.fail_on_marker, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_marker_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect todo policy failed: {message}"
        )));
    }

    Ok(())
}
