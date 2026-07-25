use anyhow::Result;

use crate::application::usecase::unused_package_report::{
    UnusedPackagePolicyOptions, analyze_unused_packages, collect_declared_packages,
    collect_referenced_package_names, evaluate_unused_package_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::unused_package_report::args::UnusedPackageReportArgs;
use crate::presentation::cli::unused_package_report::render::print_unused_package_report;

pub(in crate::presentation::cli) fn unused_package_report(
    args: UnusedPackageReportArgs,
) -> Result<()> {
    let mut declared = Vec::new();
    let mut referenced = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_packages(file, dialect, &tree)?);
        referenced.extend(collect_referenced_package_names(dialect, &tree)?);
    }

    let summary = analyze_unused_packages(&declared, &referenced);
    let policy = evaluate_unused_package_policy(
        UnusedPackagePolicyOptions::new(args.fail_on_unused),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unused_package_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "unused-package-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
