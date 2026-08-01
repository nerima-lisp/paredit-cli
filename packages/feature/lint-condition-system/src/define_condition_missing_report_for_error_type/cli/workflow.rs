use paredit_core_cli::CommandResult;

use crate::define_condition_missing_report_for_error_type::cli::args::DefineConditionMissingReportForErrorTypeReportArgs;
use crate::define_condition_missing_report_for_error_type::cli::render::print_define_condition_missing_report_for_error_type_report;
use crate::define_condition_missing_report_for_error_type::usecase::{
    build_define_condition_missing_report_for_error_type_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn define_condition_missing_report_for_error_type_report(
    args: DefineConditionMissingReportForErrorTypeReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_define_condition_missing_report_for_error_type_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_define_condition_missing_report_for_error_type_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "define-condition-missing-report-for-error-type-report policy failed: {message}"
        )));
    }

    Ok(())
}
