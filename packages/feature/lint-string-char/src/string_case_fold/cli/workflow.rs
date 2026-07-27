use anyhow::Result;

use crate::string_case_fold::cli::args::StringCaseFoldReportArgs;
use crate::string_case_fold::cli::render::print_string_case_fold_report;
use crate::string_case_fold::usecase::{
    StringCaseFoldPolicyOptions, collect_string_case_folds, evaluate_string_case_fold_policy,
    summarize_string_case_folds,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn string_case_fold_report(args: StringCaseFoldReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut compare_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_string_case_folds(file, dialect, &tree)?;
        compare_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_string_case_folds(compare_form_count, violations);
    let policy = evaluate_string_case_fold_policy(
        StringCaseFoldPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_string_case_fold_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "string-case-fold-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
