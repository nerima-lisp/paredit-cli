use paredit_core_cli::CommandResult;

use crate::nested_cxr::cli::args::NestedCxrReportArgs;
use crate::nested_cxr::cli::render::print_nested_cxr_report;
use crate::nested_cxr::usecase::{
    NestedCxrPolicyOptions, collect_nested_cxrs, evaluate_nested_cxr_policy, summarize_nested_cxrs,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_cxr_report(args: NestedCxrReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nested_cxrs(file, dialect, &tree)?;
        accessor_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nested_cxrs(accessor_form_count, violations);
    let policy = evaluate_nested_cxr_policy(
        NestedCxrPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nested_cxr_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-cxr-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
