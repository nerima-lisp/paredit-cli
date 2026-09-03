use paredit_core_cli::CommandResult;

use crate::single_operand_list_op::cli::args::SingleOperandListOpReportArgs;
use crate::single_operand_list_op::cli::render::print_single_operand_list_op_report;
use crate::single_operand_list_op::usecase::{
    build_single_operand_list_op_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_operand_list_op_report(args: SingleOperandListOpReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_single_operand_list_op_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_single_operand_list_op_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-operand-list-op-report policy failed: {message}"
        )));
    }

    Ok(())
}
