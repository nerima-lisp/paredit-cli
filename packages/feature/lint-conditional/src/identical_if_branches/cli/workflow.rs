use anyhow::Result;

use crate::identical_if_branches::cli::args::IdenticalIfBranchReportArgs;
use crate::identical_if_branches::cli::render::print_identical_if_branch_report;
use crate::identical_if_branches::usecase::{
    IdenticalIfBranchPolicyOptions, collect_identical_if_branches,
    evaluate_identical_if_branch_policy, summarize_identical_if_branches,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn identical_if_branch_report(args: IdenticalIfBranchReportArgs) -> Result<()> {
    let mut if_form_count = 0;
    let mut identical = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_if_form_count, file_identical) =
            collect_identical_if_branches(file, dialect, &tree)?;
        if_form_count += file_if_form_count;
        identical.extend(file_identical);
    }

    let summary = summarize_identical_if_branches(if_form_count, identical);
    let policy = evaluate_identical_if_branch_policy(
        IdenticalIfBranchPolicyOptions::new(args.fail_on_identical),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_identical_if_branch_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "identical-if-branch-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
