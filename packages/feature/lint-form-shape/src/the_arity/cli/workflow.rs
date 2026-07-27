use anyhow::Result;

use crate::the_arity::cli::args::TheArityReportArgs;
use crate::the_arity::cli::render::print_the_arity_report;
use crate::the_arity::usecase::{
    TheArityPolicyOptions, collect_the_arity_violations, evaluate_the_arity_policy,
    summarize_the_arity,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn the_arity_report(args: TheArityReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut the_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_the_form_count, file_violations) =
            collect_the_arity_violations(file, dialect, &tree)?;
        the_form_count += file_the_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_the_arity(the_form_count, violations);
    let policy =
        evaluate_the_arity_policy(TheArityPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_the_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "the-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
