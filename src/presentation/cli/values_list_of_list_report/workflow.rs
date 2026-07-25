use anyhow::Result;

use crate::application::usecase::values_list_of_list_report::{
    ValuesListOfListPolicyOptions, collect_values_list_of_lists,
    evaluate_values_list_of_list_policy, summarize_values_list_of_lists,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::values_list_of_list_report::args::ValuesListOfListReportArgs;
use crate::presentation::cli::values_list_of_list_report::render::print_values_list_of_list_report;

pub(in crate::presentation::cli) fn values_list_of_list_report(
    args: ValuesListOfListReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut values_list_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_values_list_of_lists(file, dialect, &tree)?;
        values_list_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_values_list_of_lists(values_list_form_count, violations);
    let policy = evaluate_values_list_of_list_policy(
        ValuesListOfListPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_values_list_of_list_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "values-list-of-list-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
