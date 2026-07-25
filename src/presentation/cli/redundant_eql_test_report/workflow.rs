use anyhow::Result;

use crate::application::usecase::redundant_eql_test_report::{
    RedundantEqlTestPolicyOptions, collect_redundant_eql_tests, evaluate_redundant_eql_test_policy,
    summarize_redundant_eql_tests,
};
use crate::presentation::cli::redundant_eql_test_report::args::RedundantEqlTestReportArgs;
use crate::presentation::cli::redundant_eql_test_report::render::print_redundant_eql_test_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_eql_test_report(
    args: RedundantEqlTestReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_eql_tests(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_eql_tests(call_form_count, violations);
    let policy = evaluate_redundant_eql_test_policy(
        RedundantEqlTestPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_eql_test_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-eql-test-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
