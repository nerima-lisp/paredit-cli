use paredit_core_cli::CommandResult;

use crate::exhaustive_case_otherwise::cli::args::ExhaustiveCaseOtherwiseReportArgs;
use crate::exhaustive_case_otherwise::cli::render::print_exhaustive_case_otherwise_report;
use crate::exhaustive_case_otherwise::usecase::{
    build_exhaustive_case_otherwise_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn exhaustive_case_otherwise_report(args: ExhaustiveCaseOtherwiseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_exhaustive_case_otherwise_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_exhaustive_case_otherwise_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "exhaustive-case-otherwise-report policy failed: {message}"
        )));
    }

    Ok(())
}
