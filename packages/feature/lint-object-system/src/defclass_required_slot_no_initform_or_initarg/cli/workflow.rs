use paredit_core_cli::{CliResult, CommandResult};

use crate::defclass_required_slot_no_initform_or_initarg::cli::args::DefclassRequiredSlotNoInitformOrInitargReportArgs;
use crate::defclass_required_slot_no_initform_or_initarg::cli::render::print_defclass_required_slot_no_initform_or_initarg_report;
use crate::defclass_required_slot_no_initform_or_initarg::usecase::{
    build_defclass_required_slot_no_initform_or_initarg_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn defclass_required_slot_no_initform_or_initarg_report(
    args: DefclassRequiredSlotNoInitformOrInitargReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_defclass_required_slot_no_initform_or_initarg_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_defclass_required_slot_no_initform_or_initarg_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "defclass-required-slot-no-initform-or-initarg-report policy failed: {message}"
        )));
    }

    Ok(())
}
