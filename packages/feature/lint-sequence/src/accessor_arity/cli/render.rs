use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::accessor_arity::usecase::{
    AccessorArityPolicy, AccessorAritySummary, expected_arity_phrase,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_accessor_arity_report(
    summary: &AccessorAritySummary,
    policy: &AccessorArityPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("call_count\t{}", summary.call_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\top={}\texpected={}\targuments={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.operator),
                    expected_arity_phrase(item),
                    item.argument_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "call_count": summary.call_count,
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
                            "argument_count": item.argument_count,
                            "min_arity": item.min_arity,
                            "max_arity": item.max_arity,
                            "expected": expected_arity_phrase(item),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
