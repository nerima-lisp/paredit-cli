use paredit_core_cli::CommandResult;

use crate::nested_when::cli::args::NestedWhenReportArgs;
use crate::nested_when::cli::render::print_nested_when_report;
use crate::nested_when::usecase::{
    NestedWhenPolicyOptions, collect_nested_whens, evaluate_nested_when_policy,
    summarize_nested_whens,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_when_report(args: NestedWhenReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut when_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nested_whens(file, dialect, &tree)?;
        when_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nested_whens(when_form_count, violations);
    let policy = evaluate_nested_when_policy(
        NestedWhenPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nested_when_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-when-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
