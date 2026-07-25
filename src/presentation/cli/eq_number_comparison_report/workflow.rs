use anyhow::Result;

use crate::application::usecase::eq_number_comparison_report::{
    EqNumberComparisonPolicyOptions, collect_eq_number_comparisons,
    evaluate_eq_number_comparison_policy, summarize_eq_number_comparisons,
};
use crate::presentation::cli::eq_number_comparison_report::args::EqNumberComparisonReportArgs;
use crate::presentation::cli::eq_number_comparison_report::render::print_eq_number_comparison_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn eq_number_comparison_report(
    args: EqNumberComparisonReportArgs,
) -> Result<()> {
    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_comparison_form_count, file_violations) =
            collect_eq_number_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_comparison_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eq_number_comparisons(comparison_form_count, violations);
    let policy = evaluate_eq_number_comparison_policy(
        EqNumberComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eq_number_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "eq-number-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
