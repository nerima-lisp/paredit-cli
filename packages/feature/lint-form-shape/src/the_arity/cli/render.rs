use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::the_arity::usecase::{TheArityPolicy, TheAritySummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_the_arity_report(
    summary: &TheAritySummary,
    policy: &TheArityPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("the_form_count\t{}", summary.the_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\targuments={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.argument_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "the_form_count": summary.the_form_count,
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
                            "argument_count": item.argument_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
