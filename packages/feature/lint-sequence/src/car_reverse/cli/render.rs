use anyhow::Result;
use serde_json::json;

use crate::application::usecase::car_reverse_report::{CarReversePolicy, CarReverseSummary};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_car_reverse_report(
    summary: &CarReverseSummary,
    policy: &CarReversePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("accessor_form_count\t{}", summary.accessor_form_count);
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
                    "accessor_form_count": summary.accessor_form_count,
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
                            "accessor_span": {
                                "start": item.accessor_span.start().get(),
                                "end": item.accessor_span.end().get(),
                            },
                            "list_span": {
                                "start": item.list_span.start().get(),
                                "end": item.list_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
