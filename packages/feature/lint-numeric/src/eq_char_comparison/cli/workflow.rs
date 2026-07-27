use anyhow::Result;

use crate::application::usecase::eq_char_comparison_report::{
    EqCharComparisonPolicyOptions, collect_eq_char_comparisons, evaluate_eq_char_comparison_policy,
    summarize_eq_char_comparisons,
};
use crate::presentation::cli::eq_char_comparison_report::args::EqCharComparisonReportArgs;
use crate::presentation::cli::eq_char_comparison_report::render::print_eq_char_comparison_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn eq_char_comparison_report(
    args: EqCharComparisonReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "eq-char-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
