use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::defpackage_quoted::usecase::{DefpackageQuotedPolicy, DefpackageQuotedSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_defpackage_quoted_report(
    summary: &DefpackageQuotedSummary,
    policy: &DefpackageQuotedPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("defpackage_form_count\t{}", summary.defpackage_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.clause,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "defpackage_form_count": summary.defpackage_form_count,
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
                            "clause": item.clause,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "designator_span": {
                                "start": item.designator_span.start().get(),
                                "end": item.designator_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
