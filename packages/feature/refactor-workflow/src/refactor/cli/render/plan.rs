use super::super::super::*;
use super::super::types::plan::RefactorPlan;

pub(in crate::presentation::cli) fn print_refactor_plan(
    plan: &RefactorPlan,
    output: OutputFormat,
) -> Result<()> {
    let mut summary = summarize_impact_reports(&plan.files);
    summary.safe_to_automate = !plan.gates.iter().any(|gate| gate.blocks_automation);

    match output {
        OutputFormat::Text => {
            println!("operation\t{}", plan.operation.label());
            println!("symbol\t{}", safe_text!(plan.symbol));
            println!("target_kind\t{}", plan.target_kind.label());
            if let Some(workspace) = &plan.workspace {
                println!(
                    "workspace_roots\t{}",
                    safe_text!(
                        workspace
                            .roots
                            .iter()
                            .map(|root| root.display().to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                );
                println!(
                    "workspace_discovered_file_count\t{}",
                    workspace.discovered_file_count
                );
                println!(
                    "workspace_skipped_unknown_count\t{}",
                    workspace.skipped_unknown_count
                );
                println!(
                    "workspace_skipped_hidden_count\t{}",
                    workspace.skipped_hidden_count
                );
                println!(
                    "workspace_skipped_generated_count\t{}",
                    workspace.skipped_generated_count
                );
                println!(
                    "workspace_skipped_symlink_count\t{}",
                    workspace.skipped_symlink_count
                );
            }
            println!("safe_to_automate\t{}", summary.safe_to_automate);
            println!("decision_status\t{}", plan.automation.status().label());
            println!("decision_reason\t{}", safe_text!(plan.automation.reason()));
            println!(
                "decision_next_action\t{}",
                safe_text!(plan.automation.next_action())
            );
            println!(
                "decision_safe_to_automate\t{}",
                plan.automation.safe_to_automate()
            );
            println!(
                "decision_policy_passed\t{}",
                plan.automation.policy_passed()
            );
            println!(
                "decision_blocking_gate_count\t{}",
                plan.automation.blocking_gate_count()
            );
            for step in plan.automation.steps() {
                println!(
                    "decision_step\t{}\tstatus={}",
                    safe_text!(step.name),
                    step.status.label()
                );
            }
            println!("policy_passed\t{}", plan.policy.passed);
            println!(
                "policy_blocking_gate_count\t{}",
                plan.policy.blocking_gate_count
            );
            println!("policy_definition_count\t{}", plan.policy.definition_count);
            println!("policy_reference_count\t{}", plan.policy.reference_count);
            for violation in &plan.policy.violations {
                println!("policy_violation\t{}", safe_text!(violation));
            }
            println!(
                "risk_highest_level\t{}",
                plan.risk_summary
                    .highest_level
                    .map(|level| level.label())
                    .unwrap_or("none")
            );
            println!("risk_info_count\t{}", plan.risk_summary.info_count);
            println!("risk_warning_count\t{}", plan.risk_summary.warning_count);
            println!("risk_error_count\t{}", plan.risk_summary.error_count);
            println!("risk_blocking_count\t{}", plan.risk_summary.blocking_count);
            println!("risk_advisory_count\t{}", plan.risk_summary.advisory_count);
            println!("files\t{}", summary.file_count);
            println!("definition_count\t{}", summary.definition_count);
            println!("reference_count\t{}", summary.reference_count);
            println!("call_count\t{}", summary.call_count);
            println!("inbound_edge_count\t{}", summary.inbound_edge_count);
            println!("outbound_edge_count\t{}", summary.outbound_edge_count);
            println!(
                "non_call_reference_count\t{}",
                summary.non_call_reference_count
            );
            println!(
                "signature_mismatch_count\t{}",
                summary.signature_mismatch_count
            );
            for gate in &plan.gates {
                println!(
                    "gate\t{}\t{}\tcount={}\tblocks={}\t{}",
                    gate.level.label(),
                    safe_text!(gate.code),
                    gate.count,
                    gate.blocks_automation,
                    safe_text!(gate.message)
                );
            }
            for step in &plan.steps {
                println!(
                    "step\t{}\t{}\tcommand={}\t{}",
                    step.order,
                    safe_text!(step.action),
                    safe_text!(step.command.as_deref().unwrap_or("<manual-review>")),
                    safe_text!(step.rationale)
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "operation": plan.operation.label(),
                "symbol": plan.symbol.as_str(),
                "target_kind": plan.target_kind.label(),
                "workspace": plan.workspace.as_ref().map(|workspace| json!({
                    "roots": workspace
                        .roots
                        .iter()
                        .map(|root| root.display().to_string())
                        .collect::<Vec<_>>(),
                    "discovered_file_count": workspace.discovered_file_count,
                    "skipped": {
                        "unknown": workspace.skipped_unknown_count,
                        "hidden": workspace.skipped_hidden_count,
                        "generated": workspace.skipped_generated_count,
                        "symlink": workspace.skipped_symlink_count,
                    },
                })),
                "safe_to_automate": summary.safe_to_automate,
                "decision": {
                    "status": plan.automation.status().label(),
                    "reason": plan.automation.reason(),
                    "next_action": plan.automation.next_action(),
                    "safe_to_automate": plan.automation.safe_to_automate(),
                    "policy_passed": plan.automation.policy_passed(),
                    "blocking_gate_count": plan.automation.blocking_gate_count(),
                    "steps": plan
                        .automation
                        .steps()
                        .iter()
                        .map(|step| json!({
                            "name": step.name,
                            "status": step.status.label(),
                        }))
                        .collect::<Vec<_>>(),
                },
                "summary": {
                    "file_count": summary.file_count,
                    "definition_count": summary.definition_count,
                    "reference_count": summary.reference_count,
                    "call_count": summary.call_count,
                    "inbound_edge_count": summary.inbound_edge_count,
                    "outbound_edge_count": summary.outbound_edge_count,
                    "non_call_reference_count": summary.non_call_reference_count,
                    "signature_mismatch_count": summary.signature_mismatch_count,
                },
                "risk_summary": {
                    "highest_level": plan.risk_summary.highest_level.map(|level| level.label()),
                    "info_count": plan.risk_summary.info_count,
                    "warning_count": plan.risk_summary.warning_count,
                    "error_count": plan.risk_summary.error_count,
                    "blocking_count": plan.risk_summary.blocking_count,
                    "advisory_count": plan.risk_summary.advisory_count,
                },
                "policy": {
                    "fail_on_blocking_gate": plan.policy.fail_on_blocking_gate,
                    "require_definitions": plan.policy.require_definitions,
                    "require_references": plan.policy.require_references,
                    "blocking_gate_count": plan.policy.blocking_gate_count,
                    "definition_count": plan.policy.definition_count,
                    "reference_count": plan.policy.reference_count,
                    "passed": plan.policy.passed,
                    "violations": plan.policy.violations,
                },
                "gates": plan
                    .gates
                    .iter()
                    .map(|gate| json!({
                        "level": gate.level.label(),
                        "code": gate.code,
                        "message": gate.message.as_str(),
                        "count": gate.count,
                        "blocks_automation": gate.blocks_automation,
                    }))
                    .collect::<Vec<_>>(),
                "steps": plan
                    .steps
                    .iter()
                    .map(|step| json!({
                        "order": step.order,
                        "action": step.action,
                        "rationale": step.rationale.as_str(),
                        "command": step.command.as_deref(),
                    }))
                    .collect::<Vec<_>>(),
                "files": plan
                    .files
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
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
