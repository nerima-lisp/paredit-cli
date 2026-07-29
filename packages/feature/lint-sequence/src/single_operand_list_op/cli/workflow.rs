use paredit_core_cli::CommandResult;

use crate::single_operand_list_op::cli::args::SingleOperandListOpReportArgs;
use crate::single_operand_list_op::cli::render::print_single_operand_list_op_report;
use crate::single_operand_list_op::usecase::{
    SingleOperandListOpPolicyOptions, collect_single_operand_list_ops,
    evaluate_single_operand_list_op_policy, summarize_single_operand_list_ops,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_operand_list_op_report(args: SingleOperandListOpReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut list_op_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_single_operand_list_ops(file, dialect, &tree)?;
        list_op_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_operand_list_ops(list_op_form_count, violations);
    let policy = evaluate_single_operand_list_op_policy(
        SingleOperandListOpPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_operand_list_op_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-operand-list-op-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
