use paredit_core_cli::CommandResult;

use crate::recursive_lock_reentry_risk::cli::args::RecursiveLockReentryRiskReportArgs;
use crate::recursive_lock_reentry_risk::cli::render::print_recursive_lock_reentry_risk_report;
use crate::recursive_lock_reentry_risk::usecase::{
    build_recursive_lock_reentry_risk_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn recursive_lock_reentry_risk_report(
    args: RecursiveLockReentryRiskReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_recursive_lock_reentry_risk_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_recursive_lock_reentry_risk_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "recursive-lock-reentry-risk-report policy failed: {message}"
        )));
    }

    Ok(())
}
