use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::typep_predicate::usecase::{TypepPredicatePolicy, TypepPredicateSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_typep_predicate_report(
    summary: &TypepPredicateSummary,
    policy: &TypepPredicatePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("typep_form_count\t{}", summary.typep_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.predicate,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "typep_form_count": summary.typep_form_count,
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
                            "predicate": item.predicate,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "object_span": {
                                "start": item.object_span.start().get(),
                                "end": item.object_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
