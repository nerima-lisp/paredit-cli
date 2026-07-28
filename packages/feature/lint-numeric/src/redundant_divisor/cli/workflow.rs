use anyhow::Result;

use crate::redundant_divisor::cli::args::RedundantDivisorReportArgs;
use crate::redundant_divisor::cli::render::print_redundant_divisor_report;
use crate::redundant_divisor::usecase::{
    RedundantDivisorPolicyOptions, collect_redundant_divisors, evaluate_redundant_divisor_policy,
    summarize_redundant_divisors,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_divisor_report(args: RedundantDivisorReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut quotient_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_divisors(file, dialect, &tree)?;
        quotient_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_divisors(quotient_form_count, violations);
    let policy = evaluate_redundant_divisor_policy(
        RedundantDivisorPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_divisor_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-divisor-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
