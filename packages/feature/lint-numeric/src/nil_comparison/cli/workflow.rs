use paredit_core_cli::CommandResult;

use crate::nil_comparison::cli::args::NilComparisonReportArgs;
use crate::nil_comparison::cli::render::print_nil_comparison_report;
use crate::nil_comparison::usecase::{
    NilComparisonPolicyOptions, collect_nil_comparisons, evaluate_nil_comparison_policy,
    summarize_nil_comparisons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nil_comparison_report(args: NilComparisonReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nil-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
