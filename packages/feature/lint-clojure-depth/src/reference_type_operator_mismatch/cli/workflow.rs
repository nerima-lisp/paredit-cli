use paredit_core_cli::CommandResult;

use crate::reference_type_operator_mismatch::cli::args::ReferenceTypeOperatorMismatchReportArgs;
use crate::reference_type_operator_mismatch::cli::render::print_reference_type_operator_mismatch_report;
use crate::reference_type_operator_mismatch::usecase::{
    build_reference_type_operator_mismatch_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn reference_type_operator_mismatch_report(
    args: ReferenceTypeOperatorMismatchReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_reference_type_operator_mismatch_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_reference_type_operator_mismatch_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "reference-type-operator-mismatch-report policy failed: {message}"
        )));
    }

    Ok(())
}
