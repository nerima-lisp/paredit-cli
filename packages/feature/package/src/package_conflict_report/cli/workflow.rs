use anyhow::Result;

use crate::package_conflict_report::cli::args::PackageConflictReportArgs;
use crate::package_conflict_report::cli::render::print_package_conflict_report;
use crate::package_conflict_report::usecase::{
    PackageConflictPolicyOptions, analyze_package_conflicts, collect_declared_package_identifiers,
    evaluate_package_conflict_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn package_conflict_report(args: PackageConflictReportArgs) -> Result<()> {
    let mut declared = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_package_identifiers(file, dialect, &tree)?);
    }

    let summary = analyze_package_conflicts(&declared);
    let policy = evaluate_package_conflict_policy(
        PackageConflictPolicyOptions::new(args.fail_on_conflict),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_conflict_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "package-conflict-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
