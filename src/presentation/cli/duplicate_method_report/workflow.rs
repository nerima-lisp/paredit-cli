use anyhow::Result;

use crate::application::usecase::duplicate_method_report::{
    DuplicateMethodPolicyOptions, analyze_duplicate_methods, collect_declared_methods,
    evaluate_duplicate_method_policy,
};
use crate::presentation::cli::duplicate_method_report::args::DuplicateMethodReportArgs;
use crate::presentation::cli::duplicate_method_report::render::print_duplicate_method_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn duplicate_method_report(
    args: DuplicateMethodReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "duplicate-method-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
