use anyhow::Result;
use serde_json::json;

use crate::application::usecase::format_missing_destination_report::{
    FormatMissingDestinationPolicy, FormatMissingDestinationSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_format_missing_destination_report(
    summary: &FormatMissingDestinationSummary,
    policy: &FormatMissingDestinationPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("format_call_count\t{}", summary.format_call_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tliteral={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.literal),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "format_call_count": summary.format_call_count,
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
                            "literal": &item.literal,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
