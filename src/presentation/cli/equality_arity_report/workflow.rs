use anyhow::Result;

use crate::application::usecase::equality_arity_report::{
    EqualityArityPolicyOptions, collect_equality_arity_violations, evaluate_equality_arity_policy,
    summarize_equality_arity,
};
use crate::presentation::cli::equality_arity_report::args::EqualityArityReportArgs;
use crate::presentation::cli::equality_arity_report::render::print_equality_arity_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn equality_arity_report(
    args: EqualityArityReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) =
            collect_equality_arity_violations(file, dialect, &tree)?;
        call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_equality_arity(call_count, violations);
    let policy = evaluate_equality_arity_policy(
        EqualityArityPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_equality_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "equality-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
