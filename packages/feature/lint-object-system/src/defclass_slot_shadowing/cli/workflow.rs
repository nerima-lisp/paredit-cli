use paredit_core_cli::CommandResult;

use crate::defclass_slot_shadowing::cli::args::DefclassSlotShadowingReportArgs;
use crate::defclass_slot_shadowing::cli::render::print_defclass_slot_shadowing_report;
use crate::defclass_slot_shadowing::usecase::{
    build_defclass_slot_shadowing_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn defclass_slot_shadowing_report(args: DefclassSlotShadowingReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_defclass_slot_shadowing_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_defclass_slot_shadowing_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "defclass-slot-shadowing-report policy failed: {message}"
        )));
    }

    Ok(())
}
