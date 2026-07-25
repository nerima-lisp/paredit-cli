use anyhow::Result;

use crate::application::usecase::setf_arity_report::{
    SetfArityPolicyOptions, collect_setf_arity_violations, evaluate_setf_arity_policy,
    summarize_setf_arity_violations,
};
use crate::presentation::cli::setf_arity_report::args::SetfArityReportArgs;
use crate::presentation::cli::setf_arity_report::render::print_setf_arity_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn setf_arity_report(args: SetfArityReportArgs) -> Result<()> {
    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_assignment_form_count, file_violations) =
            collect_setf_arity_violations(file, dialect, &tree)?;
        assignment_form_count += file_assignment_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_setf_arity_violations(assignment_form_count, violations);
    let policy = evaluate_setf_arity_policy(
        SetfArityPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_setf_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "setf-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
