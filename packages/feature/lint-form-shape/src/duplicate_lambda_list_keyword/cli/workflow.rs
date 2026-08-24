use paredit_core_cli::{CliResult, CommandResult};

use crate::duplicate_lambda_list_keyword::cli::args::DuplicateLambdaListKeywordReportArgs;
use crate::duplicate_lambda_list_keyword::cli::render::print_duplicate_lambda_list_keyword_report;
use crate::duplicate_lambda_list_keyword::usecase::{
    build_duplicate_lambda_list_keyword_report, evaluate_fail_on_duplicate_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn duplicate_lambda_list_keyword_report(
    args: DuplicateLambdaListKeywordReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_duplicate_lambda_list_keyword_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_duplicate_policy(args.fail_on_duplicate, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplicate_lambda_list_keyword_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-lambda-list-keyword-report policy failed: {message}"
        )));
    }

    Ok(())
}
