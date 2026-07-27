use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::application::usecase::dependency_report::DependencyKind;
use crate::presentation::cli::args::OutputFormat;
use crate::presentation::cli::dependency_report::types::DependencyReportFile;

pub(super) fn print_dependency_report(
    reports: &[DependencyReportFile],
    output: OutputFormat,
) -> Result<()> {
    let dependency_count = reports
        .iter()
        .map(|report| report.dependencies.len())
        .sum::<usize>();
    let mut by_kind = BTreeMap::<DependencyKind, usize>::new();
    let mut by_target = BTreeMap::<String, usize>::new();

    for dependency in reports.iter().flat_map(|report| &report.dependencies) {
        *by_kind.entry(dependency.kind).or_default() += 1;
        *by_target.entry(dependency.target.clone()).or_default() += 1;
    }

    match output {
        OutputFormat::Text => {
            println!("files\t{}", reports.len());
            println!("dependency_count\t{dependency_count}");
            for (kind, count) in &by_kind {
                println!("kind\t{}\t{count}", kind.label());
            }
            for report in reports {
                println!(
                    "{}\t{}\tpackage={}\tdependencies={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    safe_text!(report.package.as_deref().unwrap_or("<none>")),
                    report.dependencies.len()
                );
                for dependency in &report.dependencies {
                    println!(
                        "\tdependency\t{}\t{}\t{}..{}\ttarget={}\tsource={}",
                        dependency.kind.label(),
                        safe_text!(dependency.path),
                        dependency.span.start().get(),
                        dependency.span.end().get(),
                        safe_text!(dependency.target),
                        safe_text!(dependency.source.as_deref().unwrap_or("<none>"))
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "file_count": reports.len(),
                "dependency_count": dependency_count,
                "by_kind": by_kind
                    .iter()
                    .map(|(kind, count)| json!({
                        "kind": kind.label(),
                        "count": count,
                    }))
                    .collect::<Vec<_>>(),
                "by_target": by_target
                    .iter()
                    .map(|(target, count)| json!({
                        "target": target.as_str(),
                        "count": count,
                    }))
                    .collect::<Vec<_>>(),
                "files": reports
                    .iter()
                    .map(|report| json!({
                        "path": report.path.display().to_string(),
                        "dialect": report.dialect.label(),
                        "package": report.package.as_deref(),
                        "dependency_count": report.dependencies.len(),
                        "dependencies": report
                            .dependencies
                            .iter()
                            .map(|dependency| json!({
                                "kind": dependency.kind.label(),
                                "target": dependency.target.as_str(),
                                "path": dependency.path.as_str(),
                                "span": {
                                    "start": dependency.span.start().get(),
                                    "end": dependency.span.end().get(),
                                },
                                "source": dependency.source.as_deref(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
