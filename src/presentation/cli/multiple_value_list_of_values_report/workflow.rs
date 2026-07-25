use anyhow::Result;

use crate::application::usecase::multiple_value_list_of_values_report::{
    MultipleValueListOfValuesPolicyOptions, collect_multiple_value_list_of_values,
    evaluate_multiple_value_list_of_values_policy, summarize_multiple_value_list_of_values,
};
use crate::presentation::cli::multiple_value_list_of_values_report::args::MultipleValueListOfValuesReportArgs;
use crate::presentation::cli::multiple_value_list_of_values_report::render::print_multiple_value_list_of_values_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn multiple_value_list_of_values_report(
    args: MultipleValueListOfValuesReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut mvl_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_multiple_value_list_of_values(file, dialect, &tree)?;
        mvl_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_multiple_value_list_of_values(mvl_form_count, violations);
    let policy = evaluate_multiple_value_list_of_values_policy(
        MultipleValueListOfValuesPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_multiple_value_list_of_values_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "multiple-value-list-of-values-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
