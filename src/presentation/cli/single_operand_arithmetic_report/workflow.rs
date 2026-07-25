use anyhow::Result;

use crate::application::usecase::single_operand_arithmetic_report::{
    SingleOperandArithmeticPolicyOptions, collect_single_operand_arithmetic,
    evaluate_single_operand_arithmetic_policy, summarize_single_operand_arithmetic,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::single_operand_arithmetic_report::args::SingleOperandArithmeticReportArgs;
use crate::presentation::cli::single_operand_arithmetic_report::render::print_single_operand_arithmetic_report;

pub(in crate::presentation::cli) fn single_operand_arithmetic_report(
    args: SingleOperandArithmeticReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "single-operand-arithmetic-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
