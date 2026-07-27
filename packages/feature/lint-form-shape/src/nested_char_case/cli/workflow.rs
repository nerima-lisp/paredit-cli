use anyhow::Result;

use crate::nested_char_case::cli::args::NestedCharCaseReportArgs;
use crate::nested_char_case::cli::render::print_nested_char_case_report;
use crate::nested_char_case::usecase::{
    NestedCharCasePolicyOptions, collect_nested_char_cases, evaluate_nested_char_case_policy,
    summarize_nested_char_cases,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_char_case_report(args: NestedCharCaseReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut char_case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nested_char_cases(file, dialect, &tree)?;
        char_case_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nested_char_cases(char_case_form_count, violations);
    let policy = evaluate_nested_char_case_policy(
        NestedCharCasePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nested_char_case_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-char-case-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
