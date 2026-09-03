use paredit_core_cli::CommandResult;

use crate::duplicate_method_report::cli::args::DuplicateMethodReportArgs;
use crate::duplicate_method_report::cli::render::print_duplicate_method_report;
use crate::duplicate_method_report::usecase::{
    DuplicateMethodPolicyOptions, analyze_duplicate_methods, collect_declared_methods,
    evaluate_duplicate_method_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_method_report(args: DuplicateMethodReportArgs) -> CommandResult {
    let mut declared = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_methods(file, dialect, &tree)?);
    }

    let summary = analyze_duplicate_methods(&declared);
    let policy = evaluate_duplicate_method_policy(
        DuplicateMethodPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_method_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-method-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
