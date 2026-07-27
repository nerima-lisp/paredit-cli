use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::nested_char_case::usecase::{NestedCharCasePolicy, NestedCharCaseSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_nested_char_case_report(
    summary: &NestedCharCaseSummary,
    policy: &NestedCharCasePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("char_case_form_count\t{}", summary.char_case_form_count);
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
                    "char_case_form_count": summary.char_case_form_count,
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
                            "outer_span": {
                                "start": item.outer_span.start().get(),
                                "end": item.outer_span.end().get(),
                            },
                            "char_span": {
                                "start": item.char_span.start().get(),
                                "end": item.char_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
