use anyhow::Result;
use serde_json::json;

use crate::application::usecase::struct_cycle_report::{StructCyclePolicy, StructCycleSummary};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_struct_cycle_report(
    summary: &StructCycleSummary,
    policy: &StructCyclePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("struct_count\t{}", summary.struct_count);
            println!("cycles\t{}", summary.cycles.len());
            if policy.fail_on_cycle {
                println!("policy\tfail_on_cycle=true\tpassed={}", policy.passed);
            }
            for cycle in &summary.cycles {
                println!(
                    "\t{}\t{}",
                    cycle.members.len(),
                    safe_text!(cycle.members.join(", "))
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "struct_count": summary.struct_count,
                    "cycle_count": summary.cycles.len(),
                    "policy": {
                        "fail_on_cycle": policy.fail_on_cycle,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "cycles": summary.cycles
                        .iter()
                        .map(|cycle| json!({
                            "members": &cycle.members,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
