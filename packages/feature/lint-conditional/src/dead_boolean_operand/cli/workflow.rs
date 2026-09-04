use paredit_core_cli::CommandResult;

use crate::dead_boolean_operand::cli::args::DeadBooleanOperandReportArgs;
use crate::dead_boolean_operand::cli::render::print_dead_boolean_operand_report;
use crate::dead_boolean_operand::usecase::{
    build_dead_boolean_operand_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn dead_boolean_operand_report(args: DeadBooleanOperandReportArgs) -> CommandResult {
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_dead_boolean_operand_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_dead_boolean_operand_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "dead-boolean-operand-report policy failed: {message}"
        )));
    }

    Ok(())
}
