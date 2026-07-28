use anyhow::Result;

use crate::eq_char_comparison::cli::args::EqCharComparisonReportArgs;
use crate::eq_char_comparison::cli::render::print_eq_char_comparison_report;
use crate::eq_char_comparison::usecase::{
    EqCharComparisonPolicyOptions, collect_eq_char_comparisons, evaluate_eq_char_comparison_policy,
    summarize_eq_char_comparisons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eq_char_comparison_report(args: EqCharComparisonReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_eq_char_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eq_char_comparisons(comparison_form_count, violations);
    let policy = evaluate_eq_char_comparison_policy(
        EqCharComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eq_char_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eq-char-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
