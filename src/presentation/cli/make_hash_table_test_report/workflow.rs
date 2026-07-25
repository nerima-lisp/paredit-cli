use anyhow::Result;

use crate::application::usecase::make_hash_table_test_report::{
    MakeHashTableTestPolicyOptions, collect_make_hash_table_tests,
    evaluate_make_hash_table_test_policy, summarize_make_hash_table_tests,
};
use crate::presentation::cli::make_hash_table_test_report::args::MakeHashTableTestReportArgs;
use crate::presentation::cli::make_hash_table_test_report::render::print_make_hash_table_test_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn make_hash_table_test_report(
    args: MakeHashTableTestReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut make_hash_table_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_make_hash_table_tests(file, dialect, &tree)?;
        make_hash_table_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_make_hash_table_tests(make_hash_table_form_count, violations);
    let policy = evaluate_make_hash_table_test_policy(
        MakeHashTableTestPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_make_hash_table_test_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "make-hash-table-test-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
