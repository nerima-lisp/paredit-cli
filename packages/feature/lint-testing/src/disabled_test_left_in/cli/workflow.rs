use paredit_core_cli::CommandResult;

use crate::disabled_test_left_in::cli::args::DisabledTestLeftInReportArgs;
use crate::disabled_test_left_in::cli::render::print_disabled_test_left_in_report;
use crate::disabled_test_left_in::usecase::{
    build_disabled_test_left_in_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn disabled_test_left_in_report(args: DisabledTestLeftInReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_disabled_test_left_in_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_disabled_test_left_in_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "disabled-test-left-in-report policy failed: {message}"
        )));
    }

    Ok(())
}
