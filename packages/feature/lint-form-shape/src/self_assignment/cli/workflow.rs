use paredit_core_cli::{CliResult, CommandResult};

use crate::self_assignment::cli::args::SelfAssignmentReportArgs;
use crate::self_assignment::cli::render::print_self_assignment_report;
use crate::self_assignment::usecase::{
    build_self_assignment_report, evaluate_fail_on_self_assignment_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn self_assignment_report(args: SelfAssignmentReportArgs) -> CommandResult {
    // Explicit files only: this command has never expanded a directory
    // argument, and the envelope does not change what it accepts.
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_self_assignment_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_self_assignment_policy(args.fail_on_self_assignment, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_self_assignment_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "self-assignment-report policy failed: {message}"
        )));
    }

    Ok(())
}
