use anyhow::Result;

use crate::case_nil_key::cli::args::CaseNilKeyReportArgs;
use crate::case_nil_key::cli::render::print_case_nil_key_report;
use crate::case_nil_key::usecase::{
    CaseNilKeyPolicyOptions, collect_case_nil_keys, evaluate_case_nil_key_policy,
    summarize_case_nil_keys,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn case_nil_key_report(args: CaseNilKeyReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_case_nil_keys(file, dialect, &tree)?;
        case_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_case_nil_keys(case_form_count, violations);
    let policy = evaluate_case_nil_key_policy(
        CaseNilKeyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_case_nil_key_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "case-nil-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
