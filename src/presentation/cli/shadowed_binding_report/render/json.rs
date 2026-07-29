use super::*;

pub(super) fn print_shadowed_binding_report(
    reports: &[ShadowedBindingReportFile],
    policy: &ShadowedBindingPolicy,
) -> CliResult<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "scope_count": policy.scope_count,
            "shadowed_count": policy.shadowed_count,
            "policy": {
                "fail_on_shadowed": policy.fail_on_shadowed,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "scope_count": report.scope_count,
                    "shadowed": report
                        .shadowed
                        .iter()
                        .map(|item| json!({
                            "name": item.name.as_str(),
                            "inner_span": {
                                "start": item.inner_span.start().get(),
                                "end": item.inner_span.end().get(),
                            },
                            "outer_span": {
                                "start": item.outer_span.start().get(),
                                "end": item.outer_span.end().get(),
                            },
                            "outer_kind": item.outer_kind.label(),
                            "outer_label": item.outer_label.as_deref(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
