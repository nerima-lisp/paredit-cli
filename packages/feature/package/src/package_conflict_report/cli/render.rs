use anyhow::Result;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::package_conflict_report::usecase::{PackageConflictPolicy, PackageConflictSummary};

pub fn print_package_conflict_report(
    summary: &PackageConflictSummary,
    policy: &PackageConflictPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "declared_identifier_count\t{}",
                summary.declared_identifier_count
            );
            println!("conflict_count\t{}", summary.conflicts.len());
            if policy.fail_on_conflict {
                println!("policy\tfail_on_conflict=true\tpassed={}", policy.passed);
            }
            for conflict in &summary.conflicts {
                for occurrence in &conflict.occurrences {
                    println!(
                        "conflict\t{}\t{}\t{}\tpackage={}\tprimary={}",
                        safe_text!(conflict.identifier),
                        safe_text!(occurrence.path.display()),
                        occurrence.span.start().get(),
                        safe_text!(occurrence.package),
                        occurrence.is_primary_name
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "declared_identifier_count": summary.declared_identifier_count,
                    "conflict_count": summary.conflicts.len(),
                    "policy": {
                        "fail_on_conflict": policy.fail_on_conflict,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "conflicts": summary.conflicts
                        .iter()
                        .map(|conflict| json!({
                            "identifier": &conflict.identifier,
                            "occurrences": conflict.occurrences
                                .iter()
                                .map(|occurrence| json!({
                                    "path": occurrence.path.display().to_string(),
                                    "span": {
                                        "start": occurrence.span.start().get(),
                                        "end": occurrence.span.end().get(),
                                    },
                                    "package": &occurrence.package,
                                    "is_primary_name": occurrence.is_primary_name,
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
