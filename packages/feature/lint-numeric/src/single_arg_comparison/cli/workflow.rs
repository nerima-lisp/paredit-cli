use paredit_core_cli::CommandResult;

use crate::single_arg_comparison::cli::args::SingleArgComparisonReportArgs;
use crate::single_arg_comparison::cli::render::print_single_arg_comparison_report;
use crate::single_arg_comparison::usecase::{
    SingleArgComparisonPolicyOptions, collect_single_arg_comparisons,
    evaluate_single_arg_comparison_policy, summarize_single_arg_comparisons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_arg_comparison_report(args: SingleArgComparisonReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_single_arg_comparisons(file, dialect, &tree)?;
        comparison_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_arg_comparisons(comparison_form_count, violations);
    let policy = evaluate_single_arg_comparison_policy(
        SingleArgComparisonPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_arg_comparison_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-arg-comparison-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
