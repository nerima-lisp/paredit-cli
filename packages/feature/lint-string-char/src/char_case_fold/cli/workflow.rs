use anyhow::Result;

use crate::application::usecase::char_case_fold_report::{
    CharCaseFoldPolicyOptions, collect_char_case_folds, evaluate_char_case_fold_policy,
    summarize_char_case_folds,
};
use crate::presentation::cli::char_case_fold_report::args::CharCaseFoldReportArgs;
use crate::presentation::cli::char_case_fold_report::render::print_char_case_fold_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn char_case_fold_report(
    args: CharCaseFoldReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut compare_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_char_case_folds(file, dialect, &tree)?;
        compare_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_char_case_folds(compare_form_count, violations);
    let policy = evaluate_char_case_fold_policy(
        CharCaseFoldPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_char_case_fold_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "char-case-fold-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
