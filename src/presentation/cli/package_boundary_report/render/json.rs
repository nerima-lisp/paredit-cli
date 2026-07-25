use super::*;

pub(super) fn print_package_boundary_report(
    reports: &[PackageBoundaryReportFile],
    policy: &PackageBoundaryPolicy,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "qualified_symbol_count": policy.qualified_symbol_count,
            "violation_count": policy.violation_count,
            "policy": {
                "fail_on_violation": policy.fail_on_violation,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "qualified_symbol_count": report.qualified_symbol_count,
                    "violations": report
                        .violations
                        .iter()
                        .map(|item| json!({
                            "path": item.path.as_str(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "reference": item.reference.as_str(),
                            "target_package": item.target_package.as_str(),
                            "current_package": item.current_package.as_deref(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );

    Ok(())
}
