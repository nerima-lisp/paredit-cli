use anyhow::Result;
use serde_json::json;

use crate::application::usecase::system_conflict_report::{
    SystemConflictPolicy, SystemConflictSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_system_conflict_report(
    summary: &SystemConflictSummary,
    policy: &SystemConflictPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("declared_count\t{}", summary.declared_count);
            println!("conflict_count\t{}", summary.conflicts.len());
            if policy.fail_on_conflict {
                println!("policy\tfail_on_conflict=true\tpassed={}", policy.passed);
            }
            for conflict in &summary.conflicts {
                for occurrence in &conflict.occurrences {
                    println!(
                        "conflict\t{}\t{}\t{}",
                        safe_text!(conflict.name),
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
                    "conflict_count": summary.conflicts.len(),
                    "policy": {
                        "fail_on_conflict": policy.fail_on_conflict,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "conflicts": summary.conflicts
                        .iter()
                        .map(|conflict| json!({
                            "name": &conflict.name,
                            "occurrences": conflict.occurrences
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
