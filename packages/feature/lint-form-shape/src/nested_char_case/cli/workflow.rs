use paredit_core_cli::CommandResult;

use crate::nested_char_case::cli::args::NestedCharCaseReportArgs;
use crate::nested_char_case::cli::render::print_nested_char_case_report;
use crate::nested_char_case::usecase::{
    build_nested_char_case_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nested_char_case_report(args: NestedCharCaseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_nested_char_case_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_nested_char_case_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nested-char-case-report policy failed: {message}"
        )));
    }

    Ok(())
}
