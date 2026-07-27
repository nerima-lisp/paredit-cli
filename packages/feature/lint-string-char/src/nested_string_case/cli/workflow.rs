use anyhow::Result;

use crate::nested_string_case::cli::args::NestedStringCaseReportArgs;
use crate::nested_string_case::cli::render::print_nested_string_case_report;
use crate::nested_string_case::usecase::{
    NestedStringCasePolicyOptions, collect_nested_string_cases, evaluate_nested_string_case_policy,
    summarize_nested_string_cases,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_string_case_report(args: NestedStringCaseReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut string_case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nested_string_cases(file, dialect, &tree)?;
        string_case_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nested_string_cases(string_case_form_count, violations);
    let policy = evaluate_nested_string_case_policy(
        NestedStringCasePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nested_string_case_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-string-case-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
