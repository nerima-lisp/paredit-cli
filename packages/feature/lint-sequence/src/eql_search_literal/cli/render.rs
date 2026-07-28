use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::eql_search_literal::usecase::{EqlSearchLiteralPolicy, EqlSearchLiteralSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_eql_search_literal_report(
    summary: &EqlSearchLiteralSummary,
    policy: &EqlSearchLiteralPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("search_call_count\t{}", summary.search_call_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}\tliteral={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.operator),
                    safe_text!(item.literal),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "search_call_count": summary.search_call_count,
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
                            "operator": &item.operator,
                            "literal": &item.literal,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
