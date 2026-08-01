use paredit_core_cli::CommandResult;

use crate::print_object_without_print_unreadable_object::cli::args::PrintObjectWithoutPrintUnreadableObjectReportArgs;
use crate::print_object_without_print_unreadable_object::cli::render::print_print_object_without_print_unreadable_object_report;
use crate::print_object_without_print_unreadable_object::usecase::{
    build_print_object_without_print_unreadable_object_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn print_object_without_print_unreadable_object_report(
    args: PrintObjectWithoutPrintUnreadableObjectReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_print_object_without_print_unreadable_object_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_print_object_without_print_unreadable_object_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "print-object-without-print-unreadable-object-report policy failed: {message}"
        )));
    }

    Ok(())
}
