use paredit_core_cli::{CliResult, CommandResult};

use crate::dead_boolean_operand::cli::args::DeadBooleanOperandReportArgs;
use crate::dead_boolean_operand::cli::render::print_dead_boolean_operand_report;
use crate::dead_boolean_operand::usecase::{
    build_dead_boolean_operand_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn dead_boolean_operand_report(args: DeadBooleanOperandReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_dead_boolean_operand_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

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
