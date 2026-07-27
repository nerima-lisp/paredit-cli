use anyhow::Result;

use crate::application::usecase::typecase_nil_key_report::{
    TypecaseNilKeyPolicyOptions, collect_typecase_nil_keys, evaluate_typecase_nil_key_policy,
    summarize_typecase_nil_keys,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::typecase_nil_key_report::args::TypecaseNilKeyReportArgs;
use crate::presentation::cli::typecase_nil_key_report::render::print_typecase_nil_key_report;

pub(in crate::presentation::cli) fn typecase_nil_key_report(
    args: TypecaseNilKeyReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "typecase-nil-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
