use paredit_core_cli::CommandResult;

use crate::string_case_fold::cli::args::StringCaseFoldReportArgs;
use crate::string_case_fold::cli::render::print_string_case_fold_report;
use crate::string_case_fold::usecase::{
    build_string_case_fold_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn string_case_fold_report(args: StringCaseFoldReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_string_case_fold_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_string_case_fold_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "string-case-fold-report policy failed: {message}"
        )));
    }

    Ok(())
}
