use paredit_core_cli::CommandResult;

use crate::lambda_list_keyword_order::cli::args::LambdaListKeywordOrderReportArgs;
use crate::lambda_list_keyword_order::cli::render::print_lambda_list_keyword_order_report;
use crate::lambda_list_keyword_order::usecase::{
    LambdaListKeywordOrderPolicyOptions, collect_lambda_list_keyword_order,
    evaluate_lambda_list_keyword_order_policy, summarize_lambda_list_keyword_order,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn lambda_list_keyword_order_report(args: LambdaListKeywordOrderReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut definition_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_definition_count, file_violations) =
            collect_lambda_list_keyword_order(file, dialect, &tree)?;
        definition_count += file_definition_count;
        violations.extend(file_violations);
    }

    let summary = summarize_lambda_list_keyword_order(definition_count, violations);
    let policy = evaluate_lambda_list_keyword_order_policy(
        LambdaListKeywordOrderPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_lambda_list_keyword_order_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "lambda-list-keyword-order-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
