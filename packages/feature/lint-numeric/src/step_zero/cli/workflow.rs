use anyhow::Result;

use crate::step_zero::cli::args::StepZeroReportArgs;
use crate::step_zero::cli::render::print_step_zero_report;
use crate::step_zero::usecase::{
    StepZeroPolicyOptions, collect_step_zeros, evaluate_step_zero_policy, summarize_step_zeros,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn step_zero_report(args: StepZeroReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut step_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_step_zeros(file, dialect, &tree)?;
        step_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_step_zeros(step_form_count, violations);
    let policy =
        evaluate_step_zero_policy(StepZeroPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_step_zero_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "step-zero-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
