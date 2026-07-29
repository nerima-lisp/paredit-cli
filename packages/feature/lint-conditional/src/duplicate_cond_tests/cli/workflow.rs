use paredit_core_cli::CommandResult;

use crate::duplicate_cond_tests::cli::args::DuplicateCondTestReportArgs;
use crate::duplicate_cond_tests::cli::render::print_duplicate_cond_test_report;
use crate::duplicate_cond_tests::usecase::{
    DuplicateCondTestPolicyOptions, collect_duplicate_cond_tests,
    evaluate_duplicate_cond_test_policy, summarize_duplicate_cond_tests,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_cond_test_report(args: DuplicateCondTestReportArgs) -> CommandResult {
    let mut cond_form_count = 0;
    let mut duplicates = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_cond_form_count, file_duplicates) =
            collect_duplicate_cond_tests(file, dialect, &tree)?;
        cond_form_count += file_cond_form_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_cond_tests(cond_form_count, duplicates);
    let policy = evaluate_duplicate_cond_test_policy(
        DuplicateCondTestPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_cond_test_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-cond-test-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
