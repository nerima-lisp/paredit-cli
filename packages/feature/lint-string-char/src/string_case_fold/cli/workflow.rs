use anyhow::Result;

use crate::application::usecase::string_case_fold_report::{
    StringCaseFoldPolicyOptions, collect_string_case_folds, evaluate_string_case_fold_policy,
    summarize_string_case_folds,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::string_case_fold_report::args::StringCaseFoldReportArgs;
use crate::presentation::cli::string_case_fold_report::render::print_string_case_fold_report;

pub(in crate::presentation::cli) fn string_case_fold_report(
    args: StringCaseFoldReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "string-case-fold-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
