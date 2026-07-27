use anyhow::Result;

use crate::application::usecase::t_comparison_report::{
    TComparisonPolicyOptions, collect_t_comparisons, evaluate_t_comparison_policy,
    summarize_t_comparisons,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::t_comparison_report::args::TComparisonReportArgs;
use crate::presentation::cli::t_comparison_report::render::print_t_comparison_report;

pub(in crate::presentation::cli) fn t_comparison_report(args: TComparisonReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_t_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_t_comparisons(comparison_form_count, violations);
    let policy = evaluate_t_comparison_policy(
        TComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_t_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "t-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
