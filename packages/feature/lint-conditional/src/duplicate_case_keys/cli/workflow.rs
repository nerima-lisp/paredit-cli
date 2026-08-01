use paredit_core_cli::CommandResult;

use crate::duplicate_case_keys::cli::args::DuplicateCaseKeyReportArgs;
use crate::duplicate_case_keys::cli::render::print_duplicate_case_key_report;
use crate::duplicate_case_keys::usecase::{
    build_duplicate_case_key_report, evaluate_fail_on_duplicate_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_case_key_report(args: DuplicateCaseKeyReportArgs) -> CommandResult {
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_duplicate_case_key_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_duplicate_policy(args.fail_on_duplicate, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplicate_case_key_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-case-key-report policy failed: {message}"
        )));
    }

    Ok(())
}
