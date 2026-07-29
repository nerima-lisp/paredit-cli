use paredit_core_cli::CommandResult;

use crate::exhaustive_case_otherwise::cli::args::ExhaustiveCaseOtherwiseReportArgs;
use crate::exhaustive_case_otherwise::cli::render::print_exhaustive_case_otherwise_report;
use crate::exhaustive_case_otherwise::usecase::{
    ExhaustiveCaseOtherwisePolicyOptions, collect_exhaustive_case_otherwise,
    evaluate_exhaustive_case_otherwise_policy, summarize_exhaustive_case_otherwise,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn exhaustive_case_otherwise_report(args: ExhaustiveCaseOtherwiseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_exhaustive_case_otherwise(file, dialect, &tree)?;
        case_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_exhaustive_case_otherwise(case_form_count, violations);
    let policy = evaluate_exhaustive_case_otherwise_policy(
        ExhaustiveCaseOtherwisePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_exhaustive_case_otherwise_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "exhaustive-case-otherwise-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
