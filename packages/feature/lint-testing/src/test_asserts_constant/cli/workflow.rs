use paredit_core_cli::CommandResult;

use crate::test_asserts_constant::cli::args::TestAssertsConstantReportArgs;
use crate::test_asserts_constant::cli::render::print_test_asserts_constant_report;
use crate::test_asserts_constant::usecase::{
    build_test_asserts_constant_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn test_asserts_constant_report(args: TestAssertsConstantReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_test_asserts_constant_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_test_asserts_constant_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "test-asserts-constant-report policy failed: {message}"
        )));
    }

    Ok(())
}
