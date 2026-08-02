use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::nested_function_parameter_shadows_enclosing_parameter::cli::args::NestedParameterShadowReportArgs;
use crate::nested_function_parameter_shadows_enclosing_parameter::cli::render::print_nested_parameter_shadow_report;
use crate::nested_function_parameter_shadows_enclosing_parameter::usecase::{
    build_nested_parameter_shadow_report, evaluate_fail_on_violation_policy,
};

pub fn nested_parameter_shadow_report(args: NestedParameterShadowReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_nested_parameter_shadow_report(file, dialect, &tree)?);
    }

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
