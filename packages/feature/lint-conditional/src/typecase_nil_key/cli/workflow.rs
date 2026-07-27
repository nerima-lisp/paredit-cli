use anyhow::Result;

use crate::typecase_nil_key::cli::args::TypecaseNilKeyReportArgs;
use crate::typecase_nil_key::cli::render::print_typecase_nil_key_report;
use crate::typecase_nil_key::usecase::{
    TypecaseNilKeyPolicyOptions, collect_typecase_nil_keys, evaluate_typecase_nil_key_policy,
    summarize_typecase_nil_keys,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn typecase_nil_key_report(args: TypecaseNilKeyReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut typecase_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_typecase_nil_keys(file, dialect, &tree)?;
        typecase_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_typecase_nil_keys(typecase_form_count, violations);
    let policy = evaluate_typecase_nil_key_policy(
        TypecaseNilKeyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_typecase_nil_key_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "typecase-nil-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
