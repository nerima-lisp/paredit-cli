use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::format_to_string::usecase::{FormatToStringPolicy, FormatToStringSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_format_to_string_report(
    summary: &FormatToStringSummary,
    policy: &FormatToStringPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("format_form_count\t{}", summary.format_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.replacement,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "format_form_count": summary.format_form_count,
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
                            "replacement": item.replacement,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "argument_span": {
                                "start": item.argument_span.start().get(),
                                "end": item.argument_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
