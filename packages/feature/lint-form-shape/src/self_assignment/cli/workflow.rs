use anyhow::Result;

use crate::self_assignment::cli::args::SelfAssignmentReportArgs;
use crate::self_assignment::cli::render::print_self_assignment_report;
use crate::self_assignment::usecase::{
    SelfAssignmentPolicyOptions, collect_self_assignments, evaluate_self_assignment_policy,
    summarize_self_assignments,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn self_assignment_report(args: SelfAssignmentReportArgs) -> Result<()> {
    let mut assignment_form_count = 0;
    let mut self_assignments = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_assignment_form_count, file_self_assignments) =
            collect_self_assignments(file, dialect, &tree)?;
        assignment_form_count += file_assignment_form_count;
        self_assignments.extend(file_self_assignments);
    }

    let summary = summarize_self_assignments(assignment_form_count, self_assignments);
    let policy = evaluate_self_assignment_policy(
        SelfAssignmentPolicyOptions::new(args.fail_on_self_assignment),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_self_assignment_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "self-assignment-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
