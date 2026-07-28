use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::duplicate_lambda_list_keyword::usecase::{
    DuplicateLambdaListKeywordPolicy, DuplicateLambdaListKeywordSummary,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_duplicate_lambda_list_keyword_report(
    summary: &DuplicateLambdaListKeywordSummary,
    policy: &DuplicateLambdaListKeywordPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("definition_count\t{}", summary.definition_count);
            println!("duplicate_count\t{}", summary.duplicates.len());
            if policy.fail_on_duplicate {
                println!("policy\tfail_on_duplicate=true\tpassed={}", policy.passed);
            }
            for item in &summary.duplicates {
                println!(
                    "duplicate\t{}\t{}\tdefinition={}\tkeyword={}\toccurrences={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.definition),
                    safe_text!(item.keyword),
                    item.occurrence_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "definition_count": summary.definition_count,
                    "duplicate_count": summary.duplicates.len(),
                    "policy": {
                        "fail_on_duplicate": policy.fail_on_duplicate,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "duplicates": summary.duplicates
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "definition": &item.definition,
                            "keyword": &item.keyword,
                            "occurrence_count": item.occurrence_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
