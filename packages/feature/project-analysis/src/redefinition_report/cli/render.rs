use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::redefinition_report::usecase::{RedefinitionPolicy, RedefinitionSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_redefinition_report(
    summary: &RedefinitionSummary,
    policy: &RedefinitionPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("declared_count\t{}", summary.declared_count);
            println!("redefinition_count\t{}", summary.redefinitions.len());
            if policy.fail_on_redefinition {
                println!(
                    "policy\tfail_on_redefinition=true\tpassed={}",
                    policy.passed
                );
            }
            for redefinition in &summary.redefinitions {
                for occurrence in &redefinition.occurrences {
                    println!(
                        "redefinition\t{}\t{}\tcategory={}\tpackage={}\t{}",
                        safe_text!(redefinition.name),
                        redefinition.category.label(),
                        safe_text!(occurrence.path.display()),
                        safe_text!(redefinition.package.as_deref().unwrap_or("<none>")),
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
                    "redefinition_count": summary.redefinitions.len(),
                    "policy": {
                        "fail_on_redefinition": policy.fail_on_redefinition,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "redefinitions": summary.redefinitions
                        .iter()
                        .map(|redefinition| json!({
                            "name": &redefinition.name,
                            "category": redefinition.category.label(),
                            "package": redefinition.package.as_deref(),
                            "occurrences": redefinition.occurrences
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
