use anyhow::Result;

use crate::zero_divisor::cli::args::ZeroDivisorReportArgs;
use crate::zero_divisor::cli::render::print_zero_divisor_report;
use crate::zero_divisor::usecase::{
    ZeroDivisorPolicyOptions, collect_zero_divisors, evaluate_zero_divisor_policy,
    summarize_zero_divisors,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn zero_divisor_report(args: ZeroDivisorReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut division_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_zero_divisors(file, dialect, &tree)?;
        division_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_zero_divisors(division_form_count, violations);
    let policy = evaluate_zero_divisor_policy(
        ZeroDivisorPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_zero_divisor_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "zero-divisor-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
