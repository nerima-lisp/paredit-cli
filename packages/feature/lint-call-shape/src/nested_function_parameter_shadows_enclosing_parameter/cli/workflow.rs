use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};
use paredit_core_cli::{CliResult, CommandResult};

use crate::nested_function_parameter_shadows_enclosing_parameter::cli::args::NestedParameterShadowReportArgs;
use crate::nested_function_parameter_shadows_enclosing_parameter::cli::render::print_nested_parameter_shadow_report;
use crate::nested_function_parameter_shadows_enclosing_parameter::usecase::{
    build_nested_parameter_shadow_report, evaluate_fail_on_violation_policy,
};

pub fn nested_parameter_shadow_report(args: NestedParameterShadowReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_nested_parameter_shadow_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_nested_parameter_shadow_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-function-parameter-shadows-enclosing-parameter-report policy failed: {message}"
        )));
    }

    Ok(())
}
