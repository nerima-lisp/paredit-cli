use paredit_core_cli::CommandResult;

use crate::redundant_divisor::cli::args::RedundantDivisorReportArgs;
use crate::redundant_divisor::cli::render::print_redundant_divisor_report;
use crate::redundant_divisor::usecase::{
    build_redundant_divisor_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_divisor_report(args: RedundantDivisorReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_redundant_divisor_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_redundant_divisor_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-divisor-report policy failed: {message}"
        )));
    }

    Ok(())
}
