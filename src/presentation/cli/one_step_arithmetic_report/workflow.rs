use anyhow::Result;

use crate::application::usecase::one_step_arithmetic_report::{
    OneStepArithmeticPolicyOptions, collect_one_step_arithmetic,
    evaluate_one_step_arithmetic_policy, summarize_one_step_arithmetic,
};
use crate::presentation::cli::one_step_arithmetic_report::args::OneStepArithmeticReportArgs;
use crate::presentation::cli::one_step_arithmetic_report::render::print_one_step_arithmetic_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn one_step_arithmetic_report(
    args: OneStepArithmeticReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_one_step_arithmetic(file, dialect, &tree)?;
        arithmetic_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_one_step_arithmetic(arithmetic_form_count, violations);
    let policy = evaluate_one_step_arithmetic_policy(
        OneStepArithmeticPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_one_step_arithmetic_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "one-step-arithmetic-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
