use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::application::usecase::dependency_report::DependencyKind;
use crate::presentation::cli::OutputFormat;
use crate::presentation::cli::dependency_report::types::DependencyReportFile;
use paredit_core_cli::report::graph::{EdgeStyle, Graph, NodeShape};

/// Draws the dependency graph: one node per package, system, or file, one edge
/// per declared dependency.
///
/// The source of an edge is the declaring package where one is known and the
/// file otherwise, because a `defpackage` in a file with no `in-package` still
/// has an owner and it is the file. Every target is drawn open: a dependency
/// names something outside the declaring unit by construction, and nothing in
/// this report establishes that the named thing was found.
pub fn dependency_drawing(reports: &[DependencyReportFile]) -> Graph {
    let mut graph = Graph::new("inspect dependencies");
    let mut seen: BTreeMap<(String, String, DependencyKind), usize> = BTreeMap::new();

    for report in reports {
        let source_label = report
            .package
            .clone()
            .unwrap_or_else(|| report.path.display().to_string());
        graph.add_node(
            source_label.clone(),
            if report.package.is_some() {
                NodeShape::Definition
            } else {
                NodeShape::Container
            },
            None,
        );
        for dependency in &report.dependencies {
            let source = dependency
                .source
                .clone()
                .unwrap_or_else(|| source_label.clone());
            *seen
                .entry((source, dependency.target.clone(), dependency.kind))
                .or_default() += 1;
        }
    }

    for ((source, target, kind), count) in seen {
        let label = if count > 1 {
            format!("{} ×{count}", kind.label())
        } else {
            kind.label().to_owned()
        };
        graph.add_edge(source, target, Some(label), EdgeStyle::External);
    }

    graph
}

pub fn print_dependency_report(
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
