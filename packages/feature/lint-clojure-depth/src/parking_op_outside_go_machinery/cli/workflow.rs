use paredit_core_cli::CommandResult;

use crate::parking_op_outside_go_machinery::cli::args::ParkingOpOutsideGoMachineryReportArgs;
use crate::parking_op_outside_go_machinery::cli::render::print_parking_op_outside_go_machinery_report;
use crate::parking_op_outside_go_machinery::usecase::{
    build_parking_op_outside_go_machinery_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn parking_op_outside_go_machinery_report(
    args: ParkingOpOutsideGoMachineryReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_parking_op_outside_go_machinery_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_parking_op_outside_go_machinery_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "parking-op-outside-go-machinery-report policy failed: {message}"
        )));
    }

    Ok(())
}
