use paredit_core_cli::CommandResult;

use crate::eql_list_comparison::cli::args::EqlListComparisonReportArgs;
use crate::eql_list_comparison::cli::render::print_eql_list_comparison_report;
use crate::eql_list_comparison::usecase::{
    EqlListComparisonPolicyOptions, collect_eql_list_comparisons,
    evaluate_eql_list_comparison_policy, summarize_eql_list_comparisons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eql_list_comparison_report(args: EqlListComparisonReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_comparison_form_count, file_violations) =
            collect_eql_list_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_comparison_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eql_list_comparisons(comparison_form_count, violations);
    let policy = evaluate_eql_list_comparison_policy(
        EqlListComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eql_list_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eql-list-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
