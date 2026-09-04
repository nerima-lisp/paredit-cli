use paredit_core_cli::CommandResult;

use crate::multiple_value_list_of_values::cli::args::MultipleValueListOfValuesReportArgs;
use crate::multiple_value_list_of_values::cli::render::print_multiple_value_list_of_values_report;
use crate::multiple_value_list_of_values::usecase::{
    build_multiple_value_list_of_values_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn multiple_value_list_of_values_report(
    args: MultipleValueListOfValuesReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_multiple_value_list_of_values_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_multiple_value_list_of_values_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "multiple-value-list-of-values-report policy failed: {message}"
        )));
    }

    Ok(())
}
