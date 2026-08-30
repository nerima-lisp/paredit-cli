use paredit_core_cli::{CliResult, CommandResult};

use crate::asdf_perform_without_call_next_method::cli::args::AsdfPerformWithoutCallNextMethodReportArgs;
use crate::asdf_perform_without_call_next_method::cli::render::print_asdf_perform_without_call_next_method_report;
use crate::asdf_perform_without_call_next_method::usecase::{
    build_asdf_perform_without_call_next_method_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn asdf_perform_without_call_next_method_report(
    args: AsdfPerformWithoutCallNextMethodReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_asdf_perform_without_call_next_method_report(
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

    print_asdf_perform_without_call_next_method_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "asdf-perform-without-call-next-method-report policy failed: {message}"
        )));
    }

    Ok(())
}
