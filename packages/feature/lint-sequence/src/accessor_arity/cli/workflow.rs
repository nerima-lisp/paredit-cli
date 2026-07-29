use paredit_core_cli::CommandResult;

use crate::accessor_arity::cli::args::AccessorArityReportArgs;
use crate::accessor_arity::cli::render::print_accessor_arity_report;
use crate::accessor_arity::usecase::{
    AccessorArityPolicyOptions, collect_accessor_arity_violations, evaluate_accessor_arity_policy,
    summarize_accessor_arity,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn accessor_arity_report(args: AccessorArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) =
            collect_accessor_arity_violations(file, dialect, &tree)?;
        call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_accessor_arity(call_count, violations);
    let policy = evaluate_accessor_arity_policy(
        AccessorArityPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_accessor_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "accessor-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
