use anyhow::Result;
use serde_json::json;

use crate::application::usecase::values_list_of_list_report::{
    ValuesListOfListPolicy, ValuesListOfListSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_values_list_of_list_report(
    summary: &ValuesListOfListSummary,
    policy: &ValuesListOfListPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("values_list_form_count\t{}", summary.values_list_form_count);
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
                    "values_list_form_count": summary.values_list_form_count,
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
                            "elements_span": match item.elements_span {
                                Some(span) => json!({
                                    "start": span.start().get(),
                                    "end": span.end().get(),
                                }),
                                None => json!(null),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
