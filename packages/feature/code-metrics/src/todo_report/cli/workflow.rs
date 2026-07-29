use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::todo_report::cli::args::TodoReportArgs;
use crate::todo_report::cli::render::print_marker_report;
use crate::todo_report::usecase::{build_todo_report, evaluate_fail_on_marker_policy};

pub fn todo_report(args: TodoReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_todo_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_marker_policy(args.fail_on_marker, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_marker_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect todo policy failed: {message}"
        )));
    }

    Ok(())
}
