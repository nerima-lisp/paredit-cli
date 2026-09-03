use paredit_core_cli::CommandResult;

use crate::redefinition_report::cli::args::RedefinitionReportArgs;
use crate::redefinition_report::cli::render::print_redefinition_report;
use crate::redefinition_report::usecase::{
    RedefinitionPolicyOptions, analyze_redefinitions, collect_declared_definitions,
    evaluate_redefinition_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn redefinition_report(args: RedefinitionReportArgs) -> CommandResult {
    let mut declared = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_definitions(file, dialect, &tree)?);
    }

    let summary = analyze_redefinitions(&declared);
    let policy = evaluate_redefinition_policy(
        RedefinitionPolicyOptions::new(args.fail_on_redefinition),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redefinition_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redefinition-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
