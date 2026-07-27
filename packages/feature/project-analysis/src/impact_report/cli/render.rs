use super::super::*;
use crate::application::usecase::call_graph_report::CallGraphEdge;
use crate::application::usecase::impact_report::{
    ImpactReportFile, ImpactReportPolicy, impact_risks, impact_status_counts,
};

pub(in crate::presentation::cli) fn print_impact_report(
    reports: &[ImpactReportFile],
    symbol: &SymbolName,
    policy: &ImpactReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    let by_status = impact_status_counts(reports);
    let risks = impact_risks(
        policy.definition_count,
        policy.inbound_edge_count,
        policy.non_call_reference_count,
        &by_status,
    );

    match output {
        OutputFormat::Text => {
            println!("symbol\t{}", safe_text!(symbol.as_str()));
            println!("risk_level\t{}", policy.risk_level.label());
            println!("files\t{}", reports.len());
            println!("definition_count\t{}", policy.definition_count);
            println!("reference_count\t{}", policy.reference_count);
            println!("call_count\t{}", policy.call_count);
            println!("inbound_edge_count\t{}", policy.inbound_edge_count);
            let outbound_edge_count = reports
                .iter()
                .map(|report| report.outbound_edges.len())
                .sum::<usize>();
            println!("outbound_edge_count\t{outbound_edge_count}");
            println!(
                "non_call_reference_count\t{}",
                policy.non_call_reference_count
            );
            println!(
                "signature_mismatch_count\t{}",
                policy.signature_mismatch_count
            );
            println!("policy_passed\t{}", policy.passed);
            for violation in &policy.violations {
                println!("policy_violation\t{}", safe_text!(violation));
            }
            for (status, count) in &by_status {
                println!("status\t{}\t{count}", status.label());
            }
            for risk in &risks {
                println!(
                    "risk\t{}\t{}\tcount={}\t{}",
                    risk.level.label(),
                    safe_text!(risk.code),
                    risk.count,
                    safe_text!(risk.message)
                );
            }
            for report in reports {
                println!(
                    "{}\t{}\tdefinitions={}\treferences={}\tcalls={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.definitions.len(),
                    report.references.len(),
                    report.calls.len()
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "symbol": symbol.as_str(),
                "riskLevel": policy.risk_level.label(),
                "file_count": reports.len(),
                "definition_count": policy.definition_count,
                "reference_count": policy.reference_count,
                "call_count": policy.call_count,
                "inbound_edge_count": policy.inbound_edge_count,
                "outbound_edge_count": reports
                    .iter()
                    .map(|report| report.outbound_edges.len())
                    .sum::<usize>(),
                "non_call_reference_count": policy.non_call_reference_count,
                "signature_mismatch_count": policy.signature_mismatch_count,
                "policy": {
                    "fail_on_risk_level": policy
                        .fail_on_risk_level
                        .map(ApplicationImpactRiskLevel::label),
                    "require_definitions": policy.require_definitions,
                    "require_references": policy.require_references,
                    "require_calls": policy.require_calls,
                    "risk_level": policy.risk_level.label(),
                    "definition_count": policy.definition_count,
                    "reference_count": policy.reference_count,
                    "call_count": policy.call_count,
                    "signature_mismatch_count": policy.signature_mismatch_count,
                    "passed": policy.passed,
                    "violations": &policy.violations,
                },
                "by_status": by_status
                    .iter()
                    .map(|(status, count)| json!({
                        "status": status.label(),
                        "count": count,
                    }))
                    .collect::<Vec<_>>(),
                "risks": risks
                    .iter()
                    .map(|risk| json!({
                        "level": risk.level.label(),
                        "code": risk.code,
                        "message": risk.message.as_str(),
                        "count": risk.count,
                    }))
                    .collect::<Vec<_>>(),
                "files": reports
                    .iter()
                    .map(|report| json!({
                        "path": report.path.display().to_string(),
                        "dialect": report.dialect.label(),
                        "package": report.package.as_deref(),
                        "definition_count": report.definitions.len(),
                        "reference_count": report.references.len(),
                        "call_count": report.calls.len(),
                        "inbound_edge_count": report.inbound_edges.len(),
                        "outbound_edge_count": report.outbound_edges.len(),
                        "non_call_reference_count": report.non_call_reference_count,
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
                                "parameterCount": definition.parameter_count,
                                "package": definition.package.as_deref(),
                            }))
                            .collect::<Vec<_>>(),
                        "references": report
                            .references
                            .iter()
                            .map(|occurrence| json!({
                                "path": occurrence.path.as_str(),
                                "span": {
                                    "start": occurrence.span.start().get(),
                                    "end": occurrence.span.end().get(),
                                },
                                "context": occurrence.context.as_ref().map(|context| json!({
                                    "path": context.path.as_str(),
                                    "span": {
                                        "start": context.span.start().get(),
                                        "end": context.span.end().get(),
                                    },
                                    "head": context.head.as_deref(),
                                    "definitionLike": context.definition_like,
                                })),
                            }))
                            .collect::<Vec<_>>(),
                        "calls": report
                            .calls
                            .iter()
                            .map(|item| json!({
                                "path": item.call.path.as_str(),
                                "span": {
                                    "start": item.call.span.start().get(),
                                    "end": item.call.span.end().get(),
                                },
                                "head": item.call.head.as_str(),
                                "argumentCount": item.call.argument_count,
                                "minParameterCount": item.expected_parameter_arity.map(|(min, _)| min),
                                "maxParameterCount": item.expected_parameter_arity.and_then(|(_, max)| max),
                                "status": item.status.label(),
                                "enclosingDefinition": item.call.enclosing_definition.as_deref(),
                            }))
                            .collect::<Vec<_>>(),
                        "inboundEdges": report
                            .inbound_edges
                            .iter()
                            .map(call_graph_edge_json)
                            .collect::<Vec<_>>(),
                        "outboundEdges": report
                            .outbound_edges
                            .iter()
                            .map(call_graph_edge_json)
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

fn call_graph_edge_json(edge: &CallGraphEdge) -> serde_json::Value {
    json!({
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
    })
}
