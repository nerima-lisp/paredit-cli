use anyhow::Result;

use crate::constant_if_test::cli::args::ConstantIfTestReportArgs;
use crate::constant_if_test::cli::render::print_constant_if_test_report;
use crate::constant_if_test::usecase::{
    ConstantIfTestPolicyOptions, collect_constant_if_tests, evaluate_constant_if_test_policy,
    summarize_constant_if_tests,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn constant_if_test_report(args: ConstantIfTestReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_constant_if_tests(file, dialect, &tree)?;
        if_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_constant_if_tests(if_form_count, violations);
    let policy = evaluate_constant_if_test_policy(
        ConstantIfTestPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_constant_if_test_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "constant-if-test-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
