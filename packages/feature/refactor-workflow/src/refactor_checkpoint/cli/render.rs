use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use super::types::{
    CheckpointRestoreResult, CreateCheckpointResult, DeleteCheckpointResult, ListCheckpointsResult,
};

pub fn print_create_checkpoint_result(
    result: &CreateCheckpointResult,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("name\t{}", safe_text!(result.name));
            println!("created_at_unix\t{}", result.created_at_unix);
            println!("replaced_existing\t{}", result.replaced_existing);
            println!("file_count\t{}", result.files.len());
            for path in &result.files {
                println!("file\t{}", safe_text!(path.display()));
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor create-checkpoint",
                "name": result.name,
                "created_at_unix": result.created_at_unix,
                "replaced_existing": result.replaced_existing,
                "files": result
                    .files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

pub fn print_list_checkpoints_result(
    result: &ListCheckpointsResult,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "checkpoints_dir\t{}",
                safe_text!(result.checkpoints_dir.display())
            );
            println!("checkpoint_count\t{}", result.checkpoints.len());
            for checkpoint in &result.checkpoints {
                println!(
                    "checkpoint\t{}\tcreated_at_unix={}\tfile_count={}",
                    safe_text!(checkpoint.name),
                    checkpoint.created_at_unix,
                    checkpoint.files.len(),
                );
                for path in &checkpoint.files {
                    println!("  file\t{}", safe_text!(path.display()));
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor list-checkpoints",
                "checkpoints_dir": result.checkpoints_dir.display().to_string(),
                "checkpoint_count": result.checkpoints.len(),
                "checkpoints": result
                    .checkpoints
                    .iter()
                    .map(|checkpoint| json!({
                        "name": checkpoint.name,
                        "created_at_unix": checkpoint.created_at_unix,
                        "files": checkpoint
                            .files
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

pub fn print_restore_checkpoint_result(
    result: &CheckpointRestoreResult,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("name\t{}", safe_text!(result.name));
            println!("created_at_unix\t{}", result.created_at_unix);
            println!("write_requested\t{}", result.write_requested);
            println!("status\t{}", status_label(result));
            println!("next_action\t{}", safe_text!(next_action(result)));
            println!("file_count\t{}", result.summary.file_count);
            println!(
                "restorable_file_count\t{}",
                result.summary.restorable_file_count
            );
            println!("blocked_file_count\t{}", result.summary.blocked_file_count);
            println!(
                "restored_file_count\t{}",
                result.summary.restored_file_count
            );
            println!("applied\t{}", result.summary.applied);
            for file in &result.files {
                println!(
                    "file\t{}\tmatches_checkpoint={}\trestored={}\tblocked={}",
                    safe_text!(file.path.display()),
                    file.matches_checkpoint,
                    file.restored,
                    safe_text!(file.blocked_reason.as_deref().unwrap_or("")),
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor restore-checkpoint",
                "name": result.name,
                "created_at_unix": result.created_at_unix,
                "write_requested": result.write_requested,
                "status": status_label(result),
                "next_action": next_action(result),
                "summary": {
                    "file_count": result.summary.file_count,
                    "restorable_file_count": result.summary.restorable_file_count,
                    "blocked_file_count": result.summary.blocked_file_count,
                    "restored_file_count": result.summary.restored_file_count,
                    "applied": result.summary.applied,
                },
                "files": result
                    .files
                    .iter()
                    .map(|file| json!({
                        "path": file.path.display().to_string(),
                        "matches_checkpoint": file.matches_checkpoint,
                        "restored": file.restored,
                        "blocked_reason": file.blocked_reason,
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

/// The single word an automation branches on.
const fn status_label(result: &CheckpointRestoreResult) -> &'static str {
    if result.summary.blocked_file_count > 0 {
        "blocked"
    } else if result.summary.file_count == 0 {
        "empty"
    } else if result.summary.applied {
        "restored"
    } else {
        "restorable"
    }
}

fn next_action(result: &CheckpointRestoreResult) -> &'static str {
    match status_label(result) {
        "blocked" => {
            "a covered file no longer matches the checkpoint; inspect the drift before restoring"
        }
        "empty" => "the checkpoint covers no files; nothing to restore",
        "restored" => "every covered file matches the checkpoint",
        _ => "re-run with --write to confirm the checkpoint still holds",
    }
}

pub fn print_delete_checkpoint_result(
    result: &DeleteCheckpointResult,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("name\t{}", safe_text!(result.name));
            println!("deleted\t{}", result.deleted);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor delete-checkpoint",
                "name": result.name,
                "deleted": result.deleted,
            }))?
        ),
    }
    Ok(())
}
