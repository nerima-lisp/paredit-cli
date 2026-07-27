use super::super::super::*;
use super::super::types::preview::{RefactorPreview, RefactorPreviewDecision};
use super::write_plan::{print_refactor_write_plan, refactor_write_plan_json};

pub(in crate::presentation::cli) fn print_refactor_preview(
    preview: &RefactorPreview,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            let write_plan = preview.write_plan();
            let writable_files = preview.writable_paths_for_write_plan(&write_plan);
            let refused_files = preview.refused_paths_for_write_plan(&write_plan);
            let decision = preview.decision_for_write_plan(&write_plan);
            println!("mode\t{}", preview.mode.label());
            println!("from\t{}", safe_text!(preview.from));
            println!("to\t{}", safe_text!(preview.to));
            println!("write_requested\t{}", preview.write_requested);
            print_refactor_write_plan(&write_plan, &writable_files, &refused_files);
            print_refactor_preview_decision(&decision);
            if let Some(workspace) = &preview.workspace {
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
            println!("files\t{}", preview.summary.file_count());
            println!(
                "changed_file_count\t{}",
                preview.summary.changed_file_count()
            );
            for changed_file in preview.summary.changed_files() {
                println!("changed_file\t{}", safe_text!(changed_file));
            }
            println!(
                "unchanged_file_count\t{}",
                preview.summary.unchanged_file_count()
            );
            println!(
                "written_file_count\t{}",
                preview.summary.written_file_count()
            );
            println!("definition_count\t{}", preview.summary.definition_count());
            println!(
                "target_occurrence_count\t{}",
                preview.summary.target_occurrence_count()
            );
            println!("edit_count\t{}", preview.summary.edit_count());
            println!("parse_error_count\t{}", preview.summary.parse_error_count());
            println!("all_outputs_parse\t{}", preview.summary.all_outputs_parse());
            println!("policy_passed\t{}", preview.policy.passed());
            let policy_summary = preview.policy.summary();
            println!("policy_violation_count\t{}", policy_summary.violation_count);
            println!("policy_write_blocked\t{}", policy_summary.write_blocked);
            println!(
                "policy_next_action\t{}",
                safe_text!(policy_summary.next_action)
            );
            for violation in preview.policy.violations() {
                println!("policy_violation\t{}", safe_text!(violation));
            }
            for file in &preview.files {
                println!(
                    "file\t{}\t{}\tchanged={}\twritten={}\tedits={}\tparse={}\tinput_hash={}\toutput_hash={}",
                    safe_text!(file.path.display()),
                    file.dialect.label(),
                    file.changed,
                    file.written,
                    file.edit_count,
                    file.output_parse_ok,
                    file.input_hash,
                    file.output_hash
                );
                for edit in &file.edits {
                    println!(
                        "edit\t{}\tstart={}\tend={}\treplacement={}",
                        safe_text!(file.path.display()),
                        edit.start(),
                        edit.end(),
                        safe_text!(edit.replacement())
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&refactor_preview_manifest_json(preview))?
            );
        }
    }

    Ok(())
}

/// Builds the preview manifest exactly as `refactor preview --output json`
/// prints it; `--manifest-out` persists these bytes so the file hash matches
/// a shell redirect of the same run.
pub(in crate::presentation::cli) fn refactor_preview_manifest_json(
    preview: &RefactorPreview,
) -> Value {
    let write_plan = preview.write_plan();
    let writable_files = preview.writable_paths_for_write_plan(&write_plan);
    let refused_files = preview.refused_paths_for_write_plan(&write_plan);
    let decision = preview.decision_for_write_plan(&write_plan);
    json!({
        "schema_version": 1,
                    "mode": preview.mode.label(),
                    "from": preview.from.as_str(),
                    "to": preview.to.as_str(),
                    "write_requested": preview.write_requested,
                    "write_plan": refactor_write_plan_json(
                        &write_plan,
                        &writable_files,
                        &refused_files
                    ),
                    "decision": refactor_preview_decision_json(&decision),
                    "workspace": preview.workspace.as_ref().map(|workspace| json!({
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
                    "summary": {
                        "file_count": preview.summary.file_count(),
                        "changed_file_count": preview.summary.changed_file_count(),
                        "changed_files": &preview.summary.changed_files(),
                        "unchanged_file_count": preview.summary.unchanged_file_count(),
                        "written_file_count": preview.summary.written_file_count(),
                        "definition_count": preview.summary.definition_count(),
                        "target_occurrence_count": preview.summary.target_occurrence_count(),
                        "edit_count": preview.summary.edit_count(),
                        "parse_error_count": preview.summary.parse_error_count(),
                        "all_outputs_parse": preview.summary.all_outputs_parse(),
                    },
                    "policy": {
                        "fail_on_no_change": preview.policy.fail_on_no_change(),
                        "fail_on_parse_error": preview.policy.fail_on_parse_error(),
                        "fail_on_target_conflict": preview.policy.fail_on_target_conflict(),
                        "require_changed_files": preview.policy.require_changed_files(),
                        "require_definitions": preview.policy.require_definitions(),
                        "require_edits": preview.policy.require_edits(),
                        "passed": preview.policy.passed(),
                        "summary": refactor_preview_policy_summary_json(preview),
                        "violations": preview.policy.violations(),
                    },
        "files": preview
            .files
            .iter()
            .map(|file| json!({
                "path": file.path.display().to_string(),
                "dialect": file.dialect.label(),
                "changed": file.changed,
                "written": file.written,
                "edit_count": file.edit_count,
                "input_bytes": file.input_bytes,
                "output_bytes": file.output_bytes,
                "output_parse_ok": file.output_parse_ok,
                "input_hash": file.input_hash.as_str(),
                "output_hash": file.output_hash.as_str(),
                "edits": file
                    .edits
                    .iter()
                    .map(|edit| json!({
                        "start": edit.start(),
                        "end": edit.end(),
                        "replacement": edit.replacement(),
                    }))
                    .collect::<Vec<_>>(),
                "preview": file.preview.as_str(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn refactor_preview_policy_summary_json(preview: &RefactorPreview) -> Value {
    let summary = preview.policy.summary();

    json!({
        "violation_count": summary.violation_count,
        "write_blocked": summary.write_blocked,
        "next_action": summary.next_action,
    })
}

fn print_refactor_preview_decision(decision: &RefactorPreviewDecision) {
    println!("decision_status\t{}", decision.status.label());
    println!("decision_reason\t{}", safe_text!(decision.status.reason()));
    println!(
        "decision_next_action\t{}",
        safe_text!(decision.status.next_action())
    );
    println!(
        "decision_write_parse_refused\t{}",
        decision.write_parse_refused()
    );
    println!("decision_apply_preview\t{}", decision.apply_preview());
    for step in decision.steps() {
        println!(
            "decision_step\t{}\tstatus={}",
            safe_text!(step.name),
            step.status.label()
        );
    }
}

fn refactor_preview_decision_json(decision: &RefactorPreviewDecision) -> Value {
    json!({
        "status": decision.status.label(),
        "reason": decision.status.reason(),
        "next_action": decision.status.next_action(),
        "write_parse_refused": decision.write_parse_refused(),
        "apply_preview": decision.apply_preview(),
        "steps": decision
            .steps()
            .iter()
            .map(|step| json!({
                "name": step.name,
                "status": step.status.label(),
            }))
            .collect::<Vec<_>>(),
    })
}
