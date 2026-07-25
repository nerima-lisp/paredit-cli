use anyhow::Result;

use crate::application::usecase::sign_comparison_report::{
    SignComparisonPolicyOptions, collect_sign_comparisons, evaluate_sign_comparison_policy,
    summarize_sign_comparisons,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::sign_comparison_report::args::SignComparisonReportArgs;
use crate::presentation::cli::sign_comparison_report::render::print_sign_comparison_report;

pub(in crate::presentation::cli) fn sign_comparison_report(
    args: SignComparisonReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_sign_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_sign_comparisons(comparison_form_count, violations);
    let policy = evaluate_sign_comparison_policy(
        SignComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_sign_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "sign-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
