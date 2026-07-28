use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::negated_when_unless::usecase::{NegatedWhenUnlessPolicy, NegatedWhenUnlessSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_negated_when_unless_report(
    summary: &NegatedWhenUnlessSummary,
    policy: &NegatedWhenUnlessPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("conditional_form_count\t{}", summary.conditional_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.head),
                    safe_text!(item.negator),
                    safe_text!(item.suggested_head),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "conditional_form_count": summary.conditional_form_count,
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
                            "head": item.head,
                            "negator": item.negator,
                            "suggested_head": item.suggested_head,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
