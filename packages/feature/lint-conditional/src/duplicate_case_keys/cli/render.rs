use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::duplicate_case_keys::usecase::{DuplicateCaseKeyPolicy, DuplicateCaseKeySummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_duplicate_case_key_report(
    summary: &DuplicateCaseKeySummary,
    policy: &DuplicateCaseKeyPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("case_form_count\t{}", summary.case_form_count);
            println!("duplicate_count\t{}", summary.duplicates.len());
            if policy.fail_on_duplicate {
                println!("policy\tfail_on_duplicate=true\tpassed={}", policy.passed);
            }
            for item in &summary.duplicates {
                println!(
                    "duplicate\t{}\t{}\thead={}\tkey={}\tcount={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.head),
                    safe_text!(item.key),
                    item.occurrence_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "case_form_count": summary.case_form_count,
                    "duplicate_count": summary.duplicates.len(),
                    "policy": {
                        "fail_on_duplicate": policy.fail_on_duplicate,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "duplicates": summary.duplicates
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "head": &item.head,
                            "key": &item.key,
                            "occurrence_count": item.occurrence_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
