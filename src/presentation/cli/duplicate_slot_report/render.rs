use paredit_core_cli::CliResult;
use serde_json::json;

use crate::application::usecase::duplicate_slot_report::{
    DuplicateSlotPolicy, DuplicateSlotSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_duplicate_slot_report(
    summary: &DuplicateSlotSummary,
    policy: &DuplicateSlotPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("definition_count\t{}", summary.definition_count);
            println!("duplicate_count\t{}", summary.duplicates.len());
            if policy.fail_on_duplicate {
                println!("policy\tfail_on_duplicate=true\tpassed={}", policy.passed);
            }
            for item in &summary.duplicates {
                println!(
                    "duplicate\t{}\t{}\towner={}\tslot={}\tcount={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.owner),
                    safe_text!(item.slot),
                    item.occurrence_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "definition_count": summary.definition_count,
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
                            "owner": &item.owner,
                            "slot": &item.slot,
                            "occurrence_count": item.occurrence_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
