use super::super::super::types::undo::RefactorUndoResult;
use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

pub fn print_refactor_undo_result(
    result: &RefactorUndoResult,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "journal_path\t{}",
                safe_text!(result.journal_path.display())
            );
            println!("operation\t{}", safe_text!(result.operation));
            println!("tool_version\t{}", safe_text!(result.tool_version));
            println!("root_enforced\t{}", result.root.enforced);
            if let Some(path) = &result.root.path {
                println!("root\t{}", safe_text!(path.display()));
            }
            println!("write_requested\t{}", result.write_requested);
            println!("status\t{}", status_label(result));
            println!("next_action\t{}", safe_text!(next_action(result)));
            println!("files\t{}", result.summary.file_count);
            println!("changed_file_count\t{}", result.summary.changed_file_count);
            println!(
                "restorable_file_count\t{}",
                result.summary.restorable_file_count
            );
            println!(
                "restored_file_count\t{}",
                result.summary.restored_file_count
            );
            println!("blocked_file_count\t{}", result.summary.blocked_file_count);
            println!("applied\t{}", result.summary.applied);
            for file in &result.files {
                println!(
                    "file\t{}\tchanges_content={}\tmatches_journal={}\trestored={}\tblocked={}",
                    safe_text!(file.path.display()),
                    file.changes_content,
                    file.current_matches_journal,
                    file.restored,
                    safe_text!(file.blocked_reason.as_deref().unwrap_or("")),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "journal_path": result.journal_path.display().to_string(),
                    "operation": result.operation,
                    "tool_version": result.tool_version,
                    "root": {
                        "enforced": result.root.enforced,
                        "path": result.root.path.as_ref().map(|path| path.display().to_string()),
                    },
                    "write_requested": result.write_requested,
                    "status": status_label(result),
                    "next_action": next_action(result),
                    "summary": {
                        "file_count": result.summary.file_count,
                        "changed_file_count": result.summary.changed_file_count,
                        "restorable_file_count": result.summary.restorable_file_count,
                        "restored_file_count": result.summary.restored_file_count,
                        "blocked_file_count": result.summary.blocked_file_count,
                        "applied": result.summary.applied,
                    },
                    "files": result
                        .files
                        .iter()
                        .map(|file| json!({
                            "path": file.path.display().to_string(),
                            "changes_content": file.changes_content,
                            "current_matches_journal": file.current_matches_journal,
                            "restored": file.restored,
                            "blocked_reason": file.blocked_reason,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// The single word an automation branches on.
const fn status_label(result: &RefactorUndoResult) -> &'static str {
    if result.summary.blocked_file_count > 0 {
        "blocked"
    } else if result.summary.changed_file_count == 0 {
        "nothing-to-undo"
    } else if result.summary.applied {
        "restored"
    } else {
        "restorable"
    }
}

/// What to do next, so a caller does not have to infer it from the counts.
fn next_action(result: &RefactorUndoResult) -> &'static str {
    match status_label(result) {
        "blocked" => {
            "a recorded file no longer matches the write; inspect the drift before undoing"
        }
        "nothing-to-undo" => "the journal records no change; nothing to do",
        "restored" => "the recorded pre-image is back on disk",
        _ => "re-run with --write to restore the recorded pre-image",
    }
}
