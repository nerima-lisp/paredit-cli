use anyhow::Result;

use crate::duplicate_case_keys::cli::args::DuplicateCaseKeyReportArgs;
use crate::duplicate_case_keys::cli::render::print_duplicate_case_key_report;
use crate::duplicate_case_keys::usecase::{
    DuplicateCaseKeyPolicyOptions, collect_duplicate_case_keys, evaluate_duplicate_case_key_policy,
    summarize_duplicate_case_keys,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_case_key_report(args: DuplicateCaseKeyReportArgs) -> Result<()> {
    let mut case_form_count = 0;
    let mut duplicates = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_case_form_count, file_duplicates) =
            collect_duplicate_case_keys(file, dialect, &tree)?;
        case_form_count += file_case_form_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_case_keys(case_form_count, duplicates);
    let policy = evaluate_duplicate_case_key_policy(
        DuplicateCaseKeyPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_case_key_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-case-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
