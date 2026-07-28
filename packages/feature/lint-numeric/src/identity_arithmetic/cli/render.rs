use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::identity_arithmetic::usecase::{IdentityArithmeticPolicy, IdentityArithmeticSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_identity_arithmetic_report(
    summary: &IdentityArithmeticSummary,
    policy: &IdentityArithmeticPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("arithmetic_form_count\t{}", summary.arithmetic_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}\tidentity={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.operator),
                    safe_text!(item.identity),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "arithmetic_form_count": summary.arithmetic_form_count,
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
                            "identity": item.identity,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
