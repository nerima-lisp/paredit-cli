use anyhow::Result;
use serde_json::json;

use crate::application::usecase::duplicate_keyword_report::{
    DuplicateKeywordPolicy, DuplicateKeywordSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_duplicate_keyword_report(
    summary: &DuplicateKeywordSummary,
    policy: &DuplicateKeywordPolicy,
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
                    "violation\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.keyword,
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
                            "keyword": item.keyword,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "duplicate_span": {
                                "start": item.duplicate_span.start().get(),
                                "end": item.duplicate_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
