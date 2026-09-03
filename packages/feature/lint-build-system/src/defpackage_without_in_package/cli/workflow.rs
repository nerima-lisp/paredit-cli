use paredit_core_cli::CommandResult;

use crate::defpackage_without_in_package::cli::args::DefpackageWithoutInPackageReportArgs;
use crate::defpackage_without_in_package::cli::render::print_defpackage_without_in_package_report;
use crate::defpackage_without_in_package::usecase::{
    build_defpackage_without_in_package_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn defpackage_without_in_package_report(
    args: DefpackageWithoutInPackageReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_defpackage_without_in_package_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_defpackage_without_in_package_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "defpackage-without-in-package-report policy failed: {message}"
        )));
    }

    Ok(())
}
