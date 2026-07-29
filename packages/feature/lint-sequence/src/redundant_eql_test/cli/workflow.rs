use paredit_core_cli::CommandResult;

use crate::redundant_eql_test::cli::args::RedundantEqlTestReportArgs;
use crate::redundant_eql_test::cli::render::print_redundant_eql_test_report;
use crate::redundant_eql_test::usecase::{
    build_redundant_eql_test_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_eql_test_report(args: RedundantEqlTestReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_redundant_eql_test_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_redundant_eql_test_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-eql-test-report policy failed: {message}"
        )));
    }

    Ok(())
}
