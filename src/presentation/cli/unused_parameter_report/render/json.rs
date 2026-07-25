use super::*;

pub(super) fn print_unused_parameter_report(
    reports: &[UnusedParameterReportFile],
    policy: &UnusedParameterReportPolicy,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "checked_definition_count": policy.checked_definition_count,
            "unused_parameter_count": policy.unused_parameter_count,
            "policy": {
                "fail_on_unused": policy.fail_on_unused,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "checked_definition_count": report.checked_definition_count,
                    "unused_parameters": report
                        .unused_parameters
                        .iter()
                        .map(|item| json!({
                            "definition_path": item.definition_path.as_str(),
                            "definition_span": {
                                "start": item.definition_span.start().get(),
                                "end": item.definition_span.end().get(),
                            },
                            "head": item.head.as_str(),
                            "definition_name": item.definition_name.as_deref(),
                            "category": item.category.label(),
                            "parameter_name": item.parameter_name.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
