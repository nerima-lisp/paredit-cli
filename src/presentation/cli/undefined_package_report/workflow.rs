use anyhow::Result;

use crate::application::usecase::undefined_package_report::{
    UndefinedPackagePolicyOptions, analyze_undefined_packages, collect_declared_package_names,
    collect_in_package_references, evaluate_undefined_package_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::undefined_package_report::args::UndefinedPackageReportArgs;
use crate::presentation::cli::undefined_package_report::render::print_undefined_package_report;

pub(in crate::presentation::cli) fn undefined_package_report(
    args: UndefinedPackageReportArgs,
) -> Result<()> {
    let mut declared = Vec::new();
    let mut referenced = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_package_names(dialect, &tree)?);
        referenced.extend(collect_in_package_references(file, dialect, &tree)?);
    }

    let summary = analyze_undefined_packages(&declared, &referenced);
    let policy = evaluate_undefined_package_policy(
        UndefinedPackagePolicyOptions::new(args.fail_on_undefined),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_undefined_package_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "undefined-package-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
