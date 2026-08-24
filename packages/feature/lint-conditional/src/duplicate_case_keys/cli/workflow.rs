use paredit_core_cli::{CliResult, CommandResult};

use crate::duplicate_case_keys::cli::args::DuplicateCaseKeyReportArgs;
use crate::duplicate_case_keys::cli::render::print_duplicate_case_key_report;
use crate::duplicate_case_keys::usecase::{
    build_duplicate_case_key_report, evaluate_fail_on_duplicate_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn duplicate_case_key_report(args: DuplicateCaseKeyReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_duplicate_case_key_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

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
