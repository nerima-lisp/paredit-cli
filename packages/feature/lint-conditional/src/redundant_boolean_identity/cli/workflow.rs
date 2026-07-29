use paredit_core_cli::CommandResult;

use crate::redundant_boolean_identity::cli::args::RedundantBooleanIdentityReportArgs;
use crate::redundant_boolean_identity::cli::render::print_redundant_boolean_identity_report;
use crate::redundant_boolean_identity::usecase::{
    RedundantBooleanIdentityPolicyOptions, collect_redundant_boolean_identities,
    evaluate_redundant_boolean_identity_policy, summarize_redundant_boolean_identities,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_boolean_identity_report(
    args: RedundantBooleanIdentityReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_redundant_boolean_identities(file, dialect, &tree)?;
        boolean_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_boolean_identities(boolean_form_count, violations);
    let policy = evaluate_redundant_boolean_identity_policy(
        RedundantBooleanIdentityPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_boolean_identity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-boolean-identity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
