use paredit_core_cli::CommandResult;

use crate::undefined_package_report::cli::args::UndefinedPackageReportArgs;
use crate::undefined_package_report::cli::render::print_undefined_package_report;
use crate::undefined_package_report::usecase::{
    UndefinedPackagePolicyOptions, analyze_undefined_packages, collect_declared_package_names,
    collect_in_package_references, evaluate_undefined_package_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn undefined_package_report(args: UndefinedPackageReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "undefined-package-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
