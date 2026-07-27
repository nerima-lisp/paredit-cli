use anyhow::Result;
use serde_json::json;

use crate::application::usecase::redundant_quote_report::{
    RedundantQuotePolicy, RedundantQuoteSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_redundant_quote_report(
    summary: &RedundantQuoteSummary,
    policy: &RedundantQuotePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("quoted_form_count\t{}", summary.quoted_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tkind={}\tliteral={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.kind,
                    safe_text!(item.literal),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "quoted_form_count": summary.quoted_form_count,
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
                            "kind": item.kind,
                            "literal": &item.literal,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
