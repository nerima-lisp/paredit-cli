use anyhow::Result;
use serde_json::json;

use crate::application::usecase::lambda_list_keyword_order_report::{
    LambdaListKeywordOrderPolicy, LambdaListKeywordOrderSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_lambda_list_keyword_order_report(
    summary: &LambdaListKeywordOrderSummary,
    policy: &LambdaListKeywordOrderPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("definition_count\t{}", summary.definition_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tdefinition={}\tkeyword={}\tafter={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.definition),
                    safe_text!(item.keyword),
                    safe_text!(item.after_keyword),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "definition_count": summary.definition_count,
                    "violation_count": summary.violations.len(),
                    "policy": {
                        "fail_on_violation": policy.fail_on_violation,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "violations": summary.violations
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "definition": &item.definition,
                            "keyword": &item.keyword,
                            "after_keyword": &item.after_keyword,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
