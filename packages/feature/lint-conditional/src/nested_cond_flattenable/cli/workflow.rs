use paredit_core_cli::CommandResult;

use crate::nested_cond_flattenable::cli::args::NestedCondFlattenableReportArgs;
use crate::nested_cond_flattenable::cli::render::print_nested_cond_flattenable_report;
use crate::nested_cond_flattenable::usecase::{
    build_nested_cond_flattenable_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_cond_flattenable_report(args: NestedCondFlattenableReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_nested_cond_flattenable_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_nested_cond_flattenable_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-cond-flattenable-report policy failed: {message}"
        )));
    }

    Ok(())
}
