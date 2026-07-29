use paredit_core_cli::CommandResult;

use crate::self_comparison::cli::args::SelfComparisonReportArgs;
use crate::self_comparison::cli::render::print_self_comparison_report;
use crate::self_comparison::usecase::{
    build_self_comparison_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn self_comparison_report(args: SelfComparisonReportArgs) -> CommandResult {
    // This command takes files, not directories: it never grew the directory
    // expansion its siblings have, and adding it here would be a user-visible
    // change of scope rather than a move onto the shared envelope.
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_self_comparison_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_self_comparison_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "self-comparison-report policy failed: {message}"
        )));
    }

    Ok(())
}
