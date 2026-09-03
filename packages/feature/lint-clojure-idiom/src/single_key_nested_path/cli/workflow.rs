use paredit_core_cli::CommandResult;

use crate::single_key_nested_path::cli::args::SingleKeyNestedPathReportArgs;
use crate::single_key_nested_path::cli::render::print_single_key_nested_path_report;
use crate::single_key_nested_path::usecase::{
    build_single_key_nested_path_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_key_nested_path_report(args: SingleKeyNestedPathReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_single_key_nested_path_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_single_key_nested_path_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-key-nested-path-report policy failed: {message}"
        )));
    }

    Ok(())
}
