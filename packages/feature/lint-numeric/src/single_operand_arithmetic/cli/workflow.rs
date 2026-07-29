use paredit_core_cli::CommandResult;

use crate::single_operand_arithmetic::cli::args::SingleOperandArithmeticReportArgs;
use crate::single_operand_arithmetic::cli::render::print_single_operand_arithmetic_report;
use crate::single_operand_arithmetic::usecase::{
    SingleOperandArithmeticPolicyOptions, collect_single_operand_arithmetic,
    evaluate_single_operand_arithmetic_policy, summarize_single_operand_arithmetic,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_operand_arithmetic_report(args: SingleOperandArithmeticReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_single_operand_arithmetic(file, dialect, &tree)?;
        arithmetic_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_operand_arithmetic(arithmetic_form_count, violations);
    let policy = evaluate_single_operand_arithmetic_policy(
        SingleOperandArithmeticPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_operand_arithmetic_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-operand-arithmetic-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
