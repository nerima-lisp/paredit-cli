use paredit_core_cli::CommandResult;

use crate::division_result_precision_loss::cli::args::DivisionResultPrecisionLossReportArgs;
use crate::division_result_precision_loss::cli::render::print_division_result_precision_loss_report;
use crate::division_result_precision_loss::usecase::{
    build_division_result_precision_loss_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn division_result_precision_loss_report(
    args: DivisionResultPrecisionLossReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_division_result_precision_loss_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_division_result_precision_loss_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "division-result-precision-loss-report policy failed: {message}"
        )));
    }

    Ok(())
}
