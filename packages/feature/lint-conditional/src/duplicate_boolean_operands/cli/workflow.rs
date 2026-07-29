use paredit_core_cli::CommandResult;

use crate::duplicate_boolean_operands::cli::args::DuplicateBooleanOperandReportArgs;
use crate::duplicate_boolean_operands::cli::render::print_duplicate_boolean_operand_report;
use crate::duplicate_boolean_operands::usecase::{
    DuplicateBooleanOperandPolicyOptions, collect_duplicate_boolean_operands,
    evaluate_duplicate_boolean_operand_policy, summarize_duplicate_boolean_operands,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_boolean_operand_report(args: DuplicateBooleanOperandReportArgs) -> CommandResult {
    let mut boolean_form_count = 0;
    let mut duplicates = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_boolean_form_count, file_duplicates) =
            collect_duplicate_boolean_operands(file, dialect, &tree)?;
        boolean_form_count += file_boolean_form_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_boolean_operands(boolean_form_count, duplicates);
    let policy = evaluate_duplicate_boolean_operand_policy(
        DuplicateBooleanOperandPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_boolean_operand_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-boolean-operand-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
