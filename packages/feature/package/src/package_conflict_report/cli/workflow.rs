use anyhow::Result;

use crate::application::usecase::package_conflict_report::{
    PackageConflictPolicyOptions, analyze_package_conflicts, collect_declared_package_identifiers,
    evaluate_package_conflict_policy,
};
use crate::presentation::cli::package_conflict_report::args::PackageConflictReportArgs;
use crate::presentation::cli::package_conflict_report::render::print_package_conflict_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn package_conflict_report(
    args: PackageConflictReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "package-conflict-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
