use paredit_core_cli::CommandResult;

use crate::eq_number_comparison::cli::args::EqNumberComparisonReportArgs;
use crate::eq_number_comparison::cli::render::print_eq_number_comparison_report;
use crate::eq_number_comparison::usecase::{
    build_eq_number_comparison_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn eq_number_comparison_report(args: EqNumberComparisonReportArgs) -> CommandResult {
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_eq_number_comparison_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_eq_number_comparison_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eq-number-comparison-report policy failed: {message}"
        )));
    }

    Ok(())
}
