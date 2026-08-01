use paredit_core_cli::CommandResult;

use crate::slot_value_bypasses_accessor::cli::args::SlotValueBypassesAccessorReportArgs;
use crate::slot_value_bypasses_accessor::cli::render::print_slot_value_bypasses_accessor_report;
use crate::slot_value_bypasses_accessor::usecase::{
    build_slot_value_bypasses_accessor_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn slot_value_bypasses_accessor_report(
    args: SlotValueBypassesAccessorReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_slot_value_bypasses_accessor_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_slot_value_bypasses_accessor_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "slot-value-bypasses-accessor-report policy failed: {message}"
        )));
    }

    Ok(())
}
