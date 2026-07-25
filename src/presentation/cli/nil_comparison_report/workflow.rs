use anyhow::Result;

use crate::application::usecase::nil_comparison_report::{
    NilComparisonPolicyOptions, collect_nil_comparisons, evaluate_nil_comparison_policy,
    summarize_nil_comparisons,
};
use crate::presentation::cli::nil_comparison_report::args::NilComparisonReportArgs;
use crate::presentation::cli::nil_comparison_report::render::print_nil_comparison_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn nil_comparison_report(
    args: NilComparisonReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nil_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nil_comparisons(comparison_form_count, violations);
    let policy = evaluate_nil_comparison_policy(
        NilComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nil_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "nil-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
