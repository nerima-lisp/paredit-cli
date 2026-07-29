use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::package_lock_report::cli::args::PackageLockReportArgs;
use crate::package_lock_report::cli::render::print_undefined_behavior_report;
use crate::package_lock_report::usecase::{
    build_package_lock_report, evaluate_fail_on_undefined_behavior_policy,
};

pub fn package_lock_report(args: PackageLockReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_package_lock_report(file, dialect, &tree));
    }

    let policy =
        evaluate_fail_on_undefined_behavior_policy(args.fail_on_undefined_behavior, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_undefined_behavior_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect package-locks policy failed: {message}"
        )));
    }

    Ok(())
}
