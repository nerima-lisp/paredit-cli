use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::redundant_divisor::usecase::{RedundantDivisorPolicy, RedundantDivisorSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_redundant_divisor_report(
    summary: &RedundantDivisorSummary,
    policy: &RedundantDivisorPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("quotient_form_count\t{}", summary.quotient_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.operator,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "quotient_form_count": summary.quotient_form_count,
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
                            "operator": item.operator,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "number_span": {
                                "start": item.number_span.start().get(),
                                "end": item.number_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
