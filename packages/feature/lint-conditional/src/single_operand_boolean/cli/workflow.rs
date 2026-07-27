use anyhow::Result;

use crate::single_operand_boolean::cli::args::SingleOperandBooleanReportArgs;
use crate::single_operand_boolean::cli::render::print_single_operand_boolean_report;
use crate::single_operand_boolean::usecase::{
    SingleOperandBooleanPolicyOptions, collect_single_operand_booleans,
    evaluate_single_operand_boolean_policy, summarize_single_operand_booleans,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_operand_boolean_report(args: SingleOperandBooleanReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_single_operand_booleans(file, dialect, &tree)?;
        boolean_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_operand_booleans(boolean_form_count, violations);
    let policy = evaluate_single_operand_boolean_policy(
        SingleOperandBooleanPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_operand_boolean_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-operand-boolean-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
