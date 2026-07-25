use anyhow::Result;
use serde_json::json;

use crate::application::usecase::package_cycle_report::{PackageCyclePolicy, PackageCycleSummary};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_package_cycle_report(
    summary: &PackageCycleSummary,
    policy: &PackageCyclePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("package_count\t{}", summary.package_count);
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
                    "package_count": summary.package_count,
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
