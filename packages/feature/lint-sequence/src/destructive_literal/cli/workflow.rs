use paredit_core_cli::CommandResult;

use crate::destructive_literal::cli::args::DestructiveLiteralReportArgs;
use crate::destructive_literal::cli::render::print_destructive_literal_report;
use crate::destructive_literal::usecase::{
    DestructiveLiteralPolicyOptions, collect_destructive_literals,
    evaluate_destructive_literal_policy, summarize_destructive_literals,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn destructive_literal_report(args: DestructiveLiteralReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut destructive_call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) =
            collect_destructive_literals(file, dialect, &tree)?;
        destructive_call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_destructive_literals(destructive_call_count, violations);
    let policy = evaluate_destructive_literal_policy(
        DestructiveLiteralPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_destructive_literal_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "destructive-literal-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
