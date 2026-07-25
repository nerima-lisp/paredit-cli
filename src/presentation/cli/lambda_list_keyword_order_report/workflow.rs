use anyhow::Result;

use crate::application::usecase::lambda_list_keyword_order_report::{
    LambdaListKeywordOrderPolicyOptions, collect_lambda_list_keyword_order,
    evaluate_lambda_list_keyword_order_policy, summarize_lambda_list_keyword_order,
};
use crate::presentation::cli::lambda_list_keyword_order_report::args::LambdaListKeywordOrderReportArgs;
use crate::presentation::cli::lambda_list_keyword_order_report::render::print_lambda_list_keyword_order_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn lambda_list_keyword_order_report(
    args: LambdaListKeywordOrderReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lambda-list-keyword-order-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
