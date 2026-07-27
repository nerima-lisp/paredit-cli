use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::application::usecase::call_graph_report::{
    CallGraphFile, CallGraphNode, CallGraphPolicy,
};
use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::OutputFormat;

pub(super) fn print_call_graph_report(
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
