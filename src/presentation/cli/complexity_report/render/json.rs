use super::*;

pub(super) fn print_complexity_report(
    reports: &[ComplexityReportFile],
    policy: &ComplexityReportPolicy,
    ranked: &[RankedComplexityEntry<'_>],
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "definition_count": policy.definition_count,
            "max_depth_overall": policy.max_depth_overall,
            "policy": {
                "fail_on_max_depth": policy.fail_on_max_depth,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "ranked": ranked
                .iter()
                .map(|entry| json!({
                    "file": entry.file.display().to_string(),
                    "dialect": entry.dialect.label(),
                    "path": entry.item.path.as_str(),
                    "head": entry.item.head.as_str(),
                    "name": entry.item.name.as_deref(),
                    "category": entry.item.category.label(),
                    "max_depth": entry.item.max_depth,
                    "atom_count": entry.item.atom_count,
                    "list_count": entry.item.list_count,
                    "complexity_score": entry.item.complexity_score,
                }))
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
                            "name": definition.name.as_deref(),
                            "category": definition.category.label(),
                            "max_depth": definition.max_depth,
                            "atom_count": definition.atom_count,
                            "list_count": definition.list_count,
                            "complexity_score": definition.complexity_score,
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
