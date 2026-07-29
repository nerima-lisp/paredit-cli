use paredit_core_cli::safe_text;
use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::call_graph_report::usecase::{CallGraphFile, CallGraphNode, CallGraphPolicy};
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::report::graph::{EdgeStyle, Graph, NodeShape};
use paredit_core_syntax::sexpr::SymbolName;

/// Draws the call graph: one node per callable, one edge per call site.
///
/// Nodes are grouped by the file that defines them, which is the grouping a
/// reader of a call graph is actually looking for — "what does this file talk
/// to" — and the only one available without a package model. A callee with no
/// definition in the scanned set is drawn open and dashed, because the edge is
/// real but its far end was never verified.
///
/// Parallel edges are collapsed: three calls to the same callee from the same
/// caller are one arrow labelled `×3`. A picture with three identical arrows
/// says nothing the label does not.
pub fn call_graph_drawing(reports: &[CallGraphFile], symbol: Option<&SymbolName>) -> Graph {
    let mut graph = Graph::new(match symbol {
        Some(symbol) => format!("inspect call-graph — {}", symbol.as_str()),
        None => "inspect call-graph".to_owned(),
    });

    for report in reports {
        let group = report.path.display().to_string();
        for definition in &report.definitions {
            if let Some(name) = &definition.name {
                graph.add_node(name.clone(), NodeShape::Definition, Some(group.clone()));
            }
        }
    }

    let mut counts: BTreeMap<(String, String, bool), usize> = BTreeMap::new();
    for edge in reports.iter().flat_map(|report| &report.edges) {
        let caller = edge.caller.clone().unwrap_or_else(|| TOP_LEVEL.to_owned());
        *counts
            .entry((caller, edge.callee.clone(), edge.internal))
            .or_default() += 1;
    }
    for ((caller, callee, internal), count) in counts {
        if caller == TOP_LEVEL {
            graph.add_node(TOP_LEVEL, NodeShape::Container, None);
        }
        graph.add_edge(
            caller,
            callee,
            (count > 1).then(|| format!("×{count}")),
            if internal {
                EdgeStyle::Internal
            } else {
                EdgeStyle::External
            },
        );
    }

    graph
}

/// The synthetic caller for a call written outside any definition.
const TOP_LEVEL: &str = "<top-level>";

pub fn print_call_graph_report(
    reports: &[CallGraphFile],
    nodes_by_name: &BTreeMap<String, CallGraphNode>,
    symbol: Option<&SymbolName>,
    include_external: bool,
    policy: &CallGraphPolicy,
    output: OutputFormat,
) -> Result<()> {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let external_edge_count = policy.edge_count.saturating_sub(policy.internal_edge_count);

    match output {
        OutputFormat::Text => {
            println!(
                "symbol\t{}",
                safe_text!(symbol.map_or("<all>", SymbolName::as_str))
            );
            println!("include_external\t{include_external}");
            println!("files\t{}", reports.len());
            println!("definition_count\t{definition_count}");
            println!("edge_count\t{}", policy.edge_count);
            println!("internal_edge_count\t{}", policy.internal_edge_count);
            println!("external_edge_count\t{external_edge_count}");
            println!("inbound_edge_count\t{}", policy.inbound_edge_count);
            println!("policy_passed\t{}", policy.passed);
            for violation in &policy.violations {
                println!("policy_violation\t{}", safe_text!(violation));
            }
            for node in nodes_by_name.values() {
                let categories = node
                    .categories
                    .iter()
                    .map(|category| category.label())
                    .collect::<Vec<_>>()
                    .join(",");
                println!(
                    "node\t{}\tdefinitions={}\tcategories={}",
                    safe_text!(node.name),
                    node.definition_count,
                    categories
                );
            }
            for report in reports {
                println!(
                    "{}\t{}\tdefinitions={}\tedges={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.definitions.len(),
                    report.edges.len()
                );
                for edge in &report.edges {
                    let caller = edge.caller.as_deref().unwrap_or("<top-level>");
                    let categories = edge
                        .callee_categories
                        .iter()
                        .map(|category| category.label())
                        .collect::<Vec<_>>()
                        .join(",");
                    println!(
                        "\tedge\t{}\t{}\t{}..{}\tcallee={}\targs={}\tinternal={}\tcategories={}",
                        safe_text!(caller),
                        safe_text!(edge.path),
                        edge.span.start().get(),
                        edge.span.end().get(),
                        safe_text!(edge.callee),
                        edge.argument_count,
                        edge.internal,
                        categories,
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "symbol": symbol.map(SymbolName::as_str),
                "includeExternal": include_external,
                "file_count": reports.len(),
                "definition_count": definition_count,
                "edge_count": policy.edge_count,
                "internal_edge_count": policy.internal_edge_count,
                "external_edge_count": external_edge_count,
                "inbound_edge_count": policy.inbound_edge_count,
                "policy": {
                    "fail_on_inbound_callers": policy.fail_on_inbound_callers,
                    "require_edges": policy.require_edges,
                    "require_internal_edges": policy.require_internal_edges,
                    "passed": policy.passed,
                    "violations": &policy.violations,
                },
                "nodes": nodes_by_name
                    .values()
                    .map(|node| json!({
                        "name": node.name.as_str(),
                        "definitionCount": node.definition_count,
                        "categories": node
                            .categories
                            .iter()
                            .map(|category| category.label())
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
                "files": reports
                    .iter()
                    .map(|report| json!({
                        "path": report.path.display().to_string(),
                        "dialect": report.dialect.label(),
                        "definition_count": report.definitions.len(),
                        "edge_count": report.edges.len(),
                        "edges": report
                            .edges
                            .iter()
                            .map(|edge| json!({
                                "caller": edge.caller.as_deref(),
                                "callee": edge.callee.as_str(),
                                "path": edge.path.as_str(),
                                "span": {
                                    "start": edge.span.start().get(),
                                    "end": edge.span.end().get(),
                                },
                                "argumentCount": edge.argument_count,
                                "internal": edge.internal,
                                "calleeCategories": edge
                                    .callee_categories
                                    .iter()
                                    .map(|category| category.label())
                                    .collect::<Vec<_>>(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
