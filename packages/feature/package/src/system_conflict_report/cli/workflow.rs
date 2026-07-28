use anyhow::Result;

use crate::system_conflict_report::cli::args::SystemConflictReportArgs;
use crate::system_conflict_report::cli::render::print_system_conflict_report;
use crate::system_conflict_report::usecase::{
    SystemConflictPolicyOptions, analyze_system_conflicts, collect_declared_systems,
    evaluate_system_conflict_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn system_conflict_report(args: SystemConflictReportArgs) -> Result<()> {
    let mut declared = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_systems(file, dialect, &tree)?);
    }

    let summary = analyze_system_conflicts(&declared);
    let policy = evaluate_system_conflict_policy(
        SystemConflictPolicyOptions::new(args.fail_on_conflict),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_system_conflict_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "system-conflict-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
