use anyhow::Result;

use crate::negated_comparison::cli::args::NegatedComparisonReportArgs;
use crate::negated_comparison::cli::render::print_negated_comparison_report;
use crate::negated_comparison::usecase::{
    NegatedComparisonPolicyOptions, collect_negated_comparisons,
    evaluate_negated_comparison_policy, summarize_negated_comparisons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn negated_comparison_report(args: NegatedComparisonReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut negation_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_negated_comparisons(file, dialect, &tree)?;
        negation_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_negated_comparisons(negation_form_count, violations);
    let policy = evaluate_negated_comparison_policy(
        NegatedComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_negated_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "negated-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
