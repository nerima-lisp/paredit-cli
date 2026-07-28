use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::nth_constant_index::usecase::{NthConstantIndexPolicy, NthConstantIndexSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_nth_constant_index_report(
    summary: &NthConstantIndexSummary,
    policy: &NthConstantIndexPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("nth_form_count\t{}", summary.nth_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tordinal={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.ordinal),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "nth_form_count": summary.nth_form_count,
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
                            "ordinal": item.ordinal,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
