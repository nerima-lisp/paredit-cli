use paredit_core_cli::CommandResult;

use crate::eq_number_comparison::cli::args::EqNumberComparisonReportArgs;
use crate::eq_number_comparison::cli::render::print_eq_number_comparison_report;
use crate::eq_number_comparison::usecase::{
    EqNumberComparisonPolicyOptions, collect_eq_number_comparisons,
    evaluate_eq_number_comparison_policy, summarize_eq_number_comparisons,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn eq_number_comparison_report(args: EqNumberComparisonReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eq-number-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
