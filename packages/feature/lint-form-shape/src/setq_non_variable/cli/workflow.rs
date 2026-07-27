use anyhow::Result;

use crate::application::usecase::setq_non_variable_report::{
    SetqNonVariablePolicyOptions, collect_setq_non_variables, evaluate_setq_non_variable_policy,
    summarize_setq_non_variables,
};
use crate::presentation::cli::setq_non_variable_report::args::SetqNonVariableReportArgs;
use crate::presentation::cli::setq_non_variable_report::render::print_setq_non_variable_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn setq_non_variable_report(
    args: SetqNonVariableReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_setq_non_variables(file, dialect, &tree)?;
        assignment_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_setq_non_variables(assignment_form_count, violations);
    let policy = evaluate_setq_non_variable_policy(
        SetqNonVariablePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_setq_non_variable_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "setq-non-variable-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
