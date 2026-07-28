use anyhow::Result;

use crate::eql_string_comparison::cli::args::EqlStringComparisonReportArgs;
use crate::eql_string_comparison::cli::render::print_eql_string_comparison_report;
use crate::eql_string_comparison::usecase::{
    EqlStringComparisonPolicyOptions, collect_eql_string_comparisons,
    evaluate_eql_string_comparison_policy, summarize_eql_string_comparisons,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn eql_string_comparison_report(args: EqlStringComparisonReportArgs) -> Result<()> {
    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_comparison_form_count, file_violations) =
            collect_eql_string_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_comparison_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eql_string_comparisons(comparison_form_count, violations);
    let policy = evaluate_eql_string_comparison_policy(
        EqlStringComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eql_string_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eql-string-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
