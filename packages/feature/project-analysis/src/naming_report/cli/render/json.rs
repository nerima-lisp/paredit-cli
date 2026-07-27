use super::*;
use anyhow::Result;
use serde_json::json;

pub fn print_naming_report(
    reports: &[NamingReportFile],
    policy: &NamingReportPolicy,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "named_definition_count": policy.named_definition_count,
            "non_idiomatic_count": policy.non_idiomatic_count,
            "policy": {
                "fail_on_non_idiomatic": policy.fail_on_non_idiomatic,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "non_idiomatic": reports
                .iter()
                .flat_map(|report| report.non_idiomatic().map(move |item| json!({
                    "file": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "path": item.path.as_str(),
                    "head": item.head.as_str(),
                    "name": item.name.as_str(),
                    "category": item.category.label(),
                    "style": item.style.label(),
                })))
                .collect::<Vec<_>>(),
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "definition_count": report.definitions.len(),
                    "definitions": report
                        .definitions
                        .iter()
                        .map(|definition| json!({
                            "path": definition.path.as_str(),
                            "span": {
                                "start": definition.span.start().get(),
                                "end": definition.span.end().get(),
                            },
                            "head": definition.head.as_str(),
                            "name": definition.name.as_str(),
                            "category": definition.category.label(),
                            "style": definition.style.label(),
                            "idiomatic": definition.style.is_idiomatic(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
