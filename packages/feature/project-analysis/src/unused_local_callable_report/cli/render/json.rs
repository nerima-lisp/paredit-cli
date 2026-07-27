use super::*;

pub(super) fn print_unused_local_callable_report(
    reports: &[UnusedLocalCallableReportFile],
    policy: &UnusedLocalCallablePolicy,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "checked_binding_count": policy.checked_binding_count,
            "unused_count": policy.unused_count,
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
                    "checked_binding_count": report.checked_binding_count,
                    "unused": report
                        .unused
                        .iter()
                        .map(|item| json!({
                            "form_span": {
                                "start": item.form_span.start().get(),
                                "end": item.form_span.end().get(),
                            },
                            "form_head": item.form_head.as_str(),
                            "name": item.name.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
