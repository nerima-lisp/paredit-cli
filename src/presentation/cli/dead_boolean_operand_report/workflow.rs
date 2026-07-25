use anyhow::Result;

use crate::application::usecase::dead_boolean_operand_report::{
    DeadBooleanOperandPolicyOptions, collect_dead_boolean_operands,
    evaluate_dead_boolean_operand_policy, summarize_dead_boolean_operands,
};
use crate::presentation::cli::dead_boolean_operand_report::args::DeadBooleanOperandReportArgs;
use crate::presentation::cli::dead_boolean_operand_report::render::print_dead_boolean_operand_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn dead_boolean_operand_report(
    args: DeadBooleanOperandReportArgs,
) -> Result<()> {
    let mut boolean_form_count = 0;
    let mut violations = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_boolean_form_count, file_violations) =
            collect_dead_boolean_operands(file, dialect, &tree)?;
        boolean_form_count += file_boolean_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_dead_boolean_operands(boolean_form_count, violations);
    let policy = evaluate_dead_boolean_operand_policy(
        DeadBooleanOperandPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_dead_boolean_operand_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "dead-boolean-operand-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
