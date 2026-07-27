use anyhow::Result;

use crate::application::usecase::redundant_identity_key_report::{
    RedundantIdentityKeyPolicyOptions, collect_redundant_identity_keys,
    evaluate_redundant_identity_key_policy, summarize_redundant_identity_keys,
};
use crate::presentation::cli::redundant_identity_key_report::args::RedundantIdentityKeyReportArgs;
use crate::presentation::cli::redundant_identity_key_report::render::print_redundant_identity_key_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_identity_key_report(
    args: RedundantIdentityKeyReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_redundant_identity_keys(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_identity_keys(call_form_count, violations);
    let policy = evaluate_redundant_identity_key_policy(
        RedundantIdentityKeyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_identity_key_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-identity-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
