use anyhow::Result;
use serde_json::json;

use crate::application::usecase::list_star_nil_report::{ListStarNilPolicy, ListStarNilSummary};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_list_star_nil_report(
    summary: &ListStarNilSummary,
    policy: &ListStarNilPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("call_form_count\t{}", summary.call_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "call_form_count": summary.call_form_count,
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
                            "head_span": {
                                "start": item.head_span.start().get(),
                                "end": item.head_span.end().get(),
                            },
                            "removal_span": {
                                "start": item.removal_span.start().get(),
                                "end": item.removal_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
