use paredit_core_cli::{CliResult, CommandResult};

use crate::identical_if_branches::cli::args::IdenticalIfBranchReportArgs;
use crate::identical_if_branches::cli::render::print_identical_if_branch_report;
use crate::identical_if_branches::usecase::{
    build_identical_if_branch_report, evaluate_fail_on_identical_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn identical_if_branch_report(args: IdenticalIfBranchReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_identical_if_branch_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_identical_policy(args.fail_on_identical, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_identical_if_branch_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "identical-if-branch-report policy failed: {message}"
        )));
    }

    Ok(())
}
