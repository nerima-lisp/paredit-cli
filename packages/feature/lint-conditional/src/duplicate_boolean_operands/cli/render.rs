use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::duplicate_boolean_operands::usecase::{
    DuplicateBooleanOperandPolicy, DuplicateBooleanOperandSummary,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_duplicate_boolean_operand_report(
    summary: &DuplicateBooleanOperandSummary,
    policy: &DuplicateBooleanOperandPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("boolean_form_count\t{}", summary.boolean_form_count);
            println!("duplicate_count\t{}", summary.duplicates.len());
            if policy.fail_on_duplicate {
                println!("policy\tfail_on_duplicate=true\tpassed={}", policy.passed);
            }
            for item in &summary.duplicates {
                println!(
                    "duplicate\t{}\t{}\thead={}\toperand={}\tcount={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.head),
                    safe_text!(item.operand),
                    item.occurrence_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "boolean_form_count": summary.boolean_form_count,
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
                            "operand": &item.operand,
                            "occurrence_count": item.occurrence_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
