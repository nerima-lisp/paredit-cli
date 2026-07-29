use paredit_core_cli::CommandResult;

use crate::duplicate_lambda_list_keyword::cli::args::DuplicateLambdaListKeywordReportArgs;
use crate::duplicate_lambda_list_keyword::cli::render::print_duplicate_lambda_list_keyword_report;
use crate::duplicate_lambda_list_keyword::usecase::{
    DuplicateLambdaListKeywordPolicyOptions, collect_duplicate_lambda_list_keywords,
    evaluate_duplicate_lambda_list_keyword_policy, summarize_duplicate_lambda_list_keywords,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn duplicate_lambda_list_keyword_report(
    args: DuplicateLambdaListKeywordReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_definition_count, file_duplicates) =
            collect_duplicate_lambda_list_keywords(file, dialect, &tree)?;
        definition_count += file_definition_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_lambda_list_keywords(definition_count, duplicates);
    let policy = evaluate_duplicate_lambda_list_keyword_policy(
        DuplicateLambdaListKeywordPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_lambda_list_keyword_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-lambda-list-keyword-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
