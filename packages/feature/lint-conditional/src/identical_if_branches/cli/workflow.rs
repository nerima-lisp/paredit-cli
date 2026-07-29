use paredit_core_cli::CommandResult;

use crate::identical_if_branches::cli::args::IdenticalIfBranchReportArgs;
use crate::identical_if_branches::cli::render::print_identical_if_branch_report;
use crate::identical_if_branches::usecase::{
    build_identical_if_branch_report, evaluate_fail_on_identical_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn identical_if_branch_report(args: IdenticalIfBranchReportArgs) -> CommandResult {
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_identical_if_branch_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_identical_policy(args.fail_on_identical, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_identical_if_branch_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "identical-if-branch-report policy failed: {message}"
        )));
    }

    Ok(())
}
