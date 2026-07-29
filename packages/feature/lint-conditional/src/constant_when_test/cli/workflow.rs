use paredit_core_cli::CommandResult;

use crate::constant_when_test::cli::args::ConstantWhenTestReportArgs;
use crate::constant_when_test::cli::render::print_constant_when_test_report;
use crate::constant_when_test::usecase::{
    ConstantWhenTestPolicyOptions, collect_constant_when_tests, evaluate_constant_when_test_policy,
    summarize_constant_when_tests,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn constant_when_test_report(args: ConstantWhenTestReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut when_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_constant_when_tests(file, dialect, &tree)?;
        when_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_constant_when_tests(when_form_count, violations);
    let policy = evaluate_constant_when_test_policy(
        ConstantWhenTestPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_constant_when_test_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "constant-when-test-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
