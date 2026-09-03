use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::intern_dynamic_package_target::cli::args::InternDynamicPackageTargetReportArgs;
use crate::intern_dynamic_package_target::cli::render::print_intern_dynamic_package_target_report;
use crate::intern_dynamic_package_target::usecase::{
    build_intern_dynamic_package_target_report, evaluate_fail_on_violation_policy,
};

pub fn intern_dynamic_package_target_report(
    args: InternDynamicPackageTargetReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_intern_dynamic_package_target_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_intern_dynamic_package_target_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "intern-dynamic-package-target-report policy failed: {message}"
        )));
    }

    Ok(())
}
