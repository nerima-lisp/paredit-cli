use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::duplicate_method_report::usecase::{DuplicateMethodPolicy, DuplicateMethodSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_duplicate_method_report(
    summary: &DuplicateMethodSummary,
    policy: &DuplicateMethodPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("declared_count\t{}", summary.declared_count);
            println!("duplicate_count\t{}", summary.duplicates.len());
            if policy.fail_on_duplicate {
                println!("policy\tfail_on_duplicate=true\tpassed={}", policy.passed);
            }
            for duplicate in &summary.duplicates {
                for occurrence in &duplicate.occurrences {
                    println!(
                        "duplicate\t{}\tqualifier={}\t{}\t{}",
                        safe_text!(duplicate.name),
                        safe_text!(duplicate.qualifier.as_deref().unwrap_or("<primary>")),
                        safe_text!(occurrence.path.display()),
                        occurrence.span.start().get(),
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "declared_count": summary.declared_count,
                    "duplicate_count": summary.duplicates.len(),
                    "policy": {
                        "fail_on_duplicate": policy.fail_on_duplicate,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "duplicates": summary.duplicates
                        .iter()
                        .map(|duplicate| json!({
                            "name": &duplicate.name,
                            "qualifier": duplicate.qualifier.as_deref(),
                            "occurrences": duplicate.occurrences
                                .iter()
                                .map(|occurrence| json!({
                                    "path": occurrence.path.display().to_string(),
                                    "span": {
                                        "start": occurrence.span.start().get(),
                                        "end": occurrence.span.end().get(),
                                    },
                                }))
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
