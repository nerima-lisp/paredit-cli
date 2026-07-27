use super::*;

pub(super) fn print_definition_report(
    reports: &[DefinitionReportFile],
    summary: &DefinitionReportSummary,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "definition_count": summary.definition_count,
            "by_category": summary
                .by_category
                .iter()
                .map(|(category, count)| json!({
                    "category": category.label(),
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "package": report.package.as_deref(),
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
                            "parameter_count": definition.parameter_count,
                            "body_form_count": definition.body_form_count,
                            "package": definition.package.as_deref(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}

pub(super) fn print_unused_definition_report(
    reports: &[UnusedDefinitionFile],
    policy: &UnusedDefinitionPolicy,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "definition_count": policy.definition_count,
            "candidate_count": policy.candidate_count,
            "actionable_candidate_count": policy.actionable_candidate_count,
            "policy": {
                "fail_on_unused": policy.fail_on_unused,
                "require_unused_definitions": policy.require_unused_definitions,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "candidates": reports
                .iter()
                .flat_map(|report| {
                    report
                        .definitions
                        .iter()
                        .filter(|item| item.references.is_empty())
                        .map(|item| {
                            let definition = &item.definition;
                            json!({
                                "file": report.path.display().to_string(),
                                "dialect": report.dialect.label(),
                                "package": report.package.as_deref(),
                                "path": definition.path.as_str(),
                                "span": {
                                    "start": definition.span.start().get(),
                                    "end": definition.span.end().get(),
                                },
                                "head": definition.head.as_str(),
                                "name": definition.name.as_deref(),
                                "category": definition.category.label(),
                                "bulk_removable": definition.category.is_bulk_removable(),
                            })
                        })
                })
                .collect::<Vec<_>>(),
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "package": report.package.as_deref(),
                    "definition_count": report.definitions.len(),
                    "candidate_count": unused_candidate_count(report),
                    "definitions": report
                        .definitions
                        .iter()
                        .map(|item| {
                            let definition = &item.definition;
                            json!({
                                "path": definition.path.as_str(),
                                "span": {
                                    "start": definition.span.start().get(),
                                    "end": definition.span.end().get(),
                                },
                                "head": definition.head.as_str(),
                                "name": definition.name.as_deref(),
                                "category": definition.category.label(),
                                "parameter_count": definition.parameter_count,
                                "body_form_count": definition.body_form_count,
                                "package": definition.package.as_deref(),
                                "reference_count": item.references.len(),
                                "unused": item.references.is_empty(),
                                "bulk_removable": definition.category.is_bulk_removable(),
                                "references": item
                                    .references
                                    .iter()
                                    .map(|reference| json!({
                                        "file": reports[reference.file_index].path.display().to_string(),
                                        "path": reference.path.as_str(),
                                        "span": {
                                            "start": reference.span.start().get(),
                                            "end": reference.span.end().get(),
                                        },
                                    }))
                                    .collect::<Vec<_>>(),
                            })
                        })
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
