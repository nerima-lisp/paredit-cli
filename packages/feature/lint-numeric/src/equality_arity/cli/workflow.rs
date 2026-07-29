use paredit_core_cli::CommandResult;

use crate::equality_arity::cli::args::EqualityArityReportArgs;
use crate::equality_arity::cli::render::print_equality_arity_report;
use crate::equality_arity::usecase::{
    EqualityArityPolicyOptions, collect_equality_arity_violations, evaluate_equality_arity_policy,
    summarize_equality_arity,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn equality_arity_report(args: EqualityArityReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "equality-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
