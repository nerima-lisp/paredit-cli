use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::serial_consistency_report::cli::args::SerialConsistencyReportArgs;
use crate::serial_consistency_report::cli::render::print_fault_report;
use crate::serial_consistency_report::usecase::{
    build_serial_consistency_report, evaluate_fail_on_fault_policy,
};

pub fn serial_consistency_report(args: SerialConsistencyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_serial_consistency_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_fault_policy(args.fail_on_fault, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_fault_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect serial-consistency policy failed: {message}"
        )));
    }

    Ok(())
}
