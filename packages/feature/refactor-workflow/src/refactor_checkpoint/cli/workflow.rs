use paredit_core_cli::ArgumentError;
use paredit_core_cli::CommandResult;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::FeatureRefusal;
use paredit_core_cli::shared::analyze_files_raw;

use paredit_core_safety::journal::{UndoJournal, UndoJournalFile};

use crate::error::RefactorContext;
use crate::refactor::cli::manifest::root::read_refactor_manifest_source;
use crate::refactor::cli::types::root::RefactorRootGuard;
use crate::refactor_checkpoint::domain::{Checkpoint, now_unix, validate_checkpoint_name};
use crate::refactor_checkpoint::store::{
    checkpoints_dir, delete_checkpoint_file, list_checkpoint_names, read_checkpoint,
    write_checkpoint,
};

use super::args::{
    CreateCheckpointArgs, DeleteCheckpointArgs, ListCheckpointsArgs, RestoreCheckpointArgs,
};
use super::render::{
    print_create_checkpoint_result, print_delete_checkpoint_result, print_list_checkpoints_result,
    print_restore_checkpoint_result,
};
use super::types::{
    CheckpointRestoreFileResult, CheckpointRestoreResult, CheckpointRestoreSummary,
    CheckpointSummary, CreateCheckpointResult, DeleteCheckpointResult, ListCheckpointsResult,
};

/// Checks a checkpoint name, converting the domain-level rejection into the
/// usage-error shape every other argument problem in this CLI reaches as.
fn require_valid_name(name: &str) -> Result<(), paredit_core_cli::CliError> {
    validate_checkpoint_name(name).map_err(|error| {
        ArgumentError::FlagCombination {
            message: error.to_string(),
        }
        .into()
    })
}

/// Records the current content of `--files` as a named checkpoint.
///
/// Each file becomes an [`UndoJournalFile`] whose before- and after-image are
/// the *same* text: nothing has changed yet, so the only honest journal entry
/// is the identity edit. See the module documentation for why that is also
/// what makes `restore-checkpoint` a byte-exact check rather than a general
/// undo.
pub fn create_checkpoint(args: CreateCheckpointArgs) -> CommandResult {
    require_valid_name(&args.name)?;

    let dir = checkpoints_dir(args.store.checkpoints_dir.clone());
    let replaced_existing = read_checkpoint(&dir, &args.name)?.is_some();
    if replaced_existing && !args.force {
        return Err(FeatureRefusal::message(
            ErrorCode::RefusalWriteTarget,
            format!(
                "checkpoint {:?} already exists; pass --force to replace it",
                args.name
            ),
        )
        .into());
    }

    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    // Deliberately not converted to the `analyze_files_raw` partial-failure
    // pattern used elsewhere in this branch: fail-fast is load-bearing here,
    // since a checkpoint must cover every requested file or record none at
    // all, not silently omit one a later restore would then have no record of.
    let mut entries = Vec::with_capacity(args.files.len());
    let mut resolved_paths = Vec::with_capacity(args.files.len());
    for path in &args.files {
        let (resolved_path, text, _expected) =
            read_refactor_manifest_source(path, root_guard.as_ref()).map_err(
                RefactorContext::new(format!("failed to read {}", path.display())),
            )?;
        let entry = UndoJournalFile::record(resolved_path.clone(), &text, &[]).map_err(
            RefactorContext::new(format!("failed to checkpoint {}", resolved_path.display())),
        )?;
        resolved_paths.push(resolved_path);
        entries.push(entry);
    }

    let created_at_unix = now_unix();
    let checkpoint = Checkpoint {
        name: args.name.clone(),
        created_at_unix,
        journal: UndoJournal::new("refactor create-checkpoint", entries),
    };
    write_checkpoint(&dir, &checkpoint)?;

    print_create_checkpoint_result(
        &CreateCheckpointResult {
            name: args.name,
            created_at_unix,
            replaced_existing,
            files: resolved_paths,
        },
        args.output,
    )?;
    Ok(())
}

/// Lists every registered checkpoint.
pub fn list_checkpoints(args: ListCheckpointsArgs) -> CommandResult {
    let dir = checkpoints_dir(args.store.checkpoints_dir.clone());
    let names = list_checkpoint_names(&dir)?;

    let mut checkpoints = Vec::with_capacity(names.len());
    for name in names {
        let Some(checkpoint) = read_checkpoint(&dir, &name)? else {
            // Deleted between the directory listing and the read: not an
            // error, just not there any more.
            continue;
        };
        checkpoints.push(CheckpointSummary {
            name: checkpoint.name,
            created_at_unix: checkpoint.created_at_unix,
            files: checkpoint
                .journal
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect(),
        });
    }

    print_list_checkpoints_result(
        &ListCheckpointsResult {
            checkpoints_dir: dir,
            checkpoints,
        },
        args.output,
    )?;
    Ok(())
}

/// Reports whether a checkpoint can be restored, and with `--write`, restores
/// it.
///
/// A file is restorable only when its current content is exactly what the
/// checkpoint recorded — [`UndoJournalFile::restore`]'s own check, reused
/// unchanged. Anything else, whether from a later `refactor apply --write` or
/// from a person editing the file directly, is refused: this command has no
/// way to tell the two apart, and silently overwriting either would be the
/// wrong default.
pub fn restore_checkpoint(args: RestoreCheckpointArgs) -> CommandResult {
    require_valid_name(&args.name)?;

    let dir = checkpoints_dir(args.store.checkpoints_dir.clone());
    let Some(checkpoint) = read_checkpoint(&dir, &args.name)? else {
        return Err(unknown_checkpoint(&dir, &args.name)?.into());
    };

    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    // The read is the I/O-bound half and is parallelized; `entry.restore`
    // (a hash comparison) is cheap enough to stay serial in the zip below. The
    // closure never fails — an unreadable file is a blocked entry, not a
    // dropped one — so `analyze_files_raw`'s order-preserving guarantee means
    // `read_results.succeeded` lines up 1:1 with `checkpoint.journal.files`.
    let paths = checkpoint
        .journal
        .files
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let read_results = analyze_files_raw(&paths, |path| {
        Result::<_, std::convert::Infallible>::Ok(
            match read_refactor_manifest_source(path, root_guard.as_ref()) {
                Ok((_, current, _)) => Ok(current),
                Err(error) => Err(format!("{error:#}")),
            },
        )
    });
    debug_assert!(read_results.failed.is_empty(), "the read never fails");

    let mut files = checkpoint
        .journal
        .files
        .iter()
        .zip(read_results.succeeded)
        .map(|(entry, read_result)| {
            let (matches_checkpoint, blocked_reason) = match read_result {
                Ok(current) => match entry.restore(&current) {
                    Ok(_) => (true, None),
                    Err(error) => (false, Some(error.to_string())),
                },
                Err(error) => (false, Some(error)),
            };
            CheckpointRestoreFileResult {
                path: entry.path.clone(),
                matches_checkpoint,
                restored: false,
                blocked_reason,
            }
        })
        .collect::<Vec<_>>();

    let blocked_file_count = files.iter().filter(|file| !file.matches_checkpoint).count();
    let restorable_file_count = files.len() - blocked_file_count;
    let can_restore = blocked_file_count == 0 && !files.is_empty();

    let mut restored_file_count = 0;
    if args.write && can_restore {
        // Every covered file already matches the checkpoint exactly — that is
        // what made it restorable — so there is nothing left to write.
        // `--write` confirms the checkpoint still holds rather than issuing a
        // no-op write, the same way `refactor undo` never rewrites a file its
        // own journal records as unchanged.
        restored_file_count = files.len();
        for file in &mut files {
            file.restored = true;
        }
    }

    let result = CheckpointRestoreResult {
        name: checkpoint.name,
        created_at_unix: checkpoint.created_at_unix,
        write_requested: args.write,
        files,
        summary: CheckpointRestoreSummary {
            file_count: restorable_file_count + blocked_file_count,
            restorable_file_count,
            blocked_file_count,
            restored_file_count,
            applied: args.write && can_restore,
        },
    };

    print_restore_checkpoint_result(&result, args.output)?;

    if args.write && !result.can_restore() {
        return Err(FeatureRefusal::message(
            ErrorCode::RefusalTargetChanged,
            format!(
                "refactor restore-checkpoint refused: {}",
                if result.summary.file_count == 0 {
                    "the checkpoint covers no files".to_owned()
                } else {
                    result.blocked_summary()
                }
            ),
        )
        .into());
    }
    Ok(())
}

/// Removes a checkpoint from the registry.
///
/// Garbage collection of checkpoints nobody deletes is out of scope here —
/// this only removes the one named.
pub fn delete_checkpoint(args: DeleteCheckpointArgs) -> CommandResult {
    require_valid_name(&args.name)?;

    let dir = checkpoints_dir(args.store.checkpoints_dir.clone());
    let deleted = delete_checkpoint_file(&dir, &args.name)?;
    if !deleted {
        return Err(unknown_checkpoint(&dir, &args.name)?.into());
    }

    print_delete_checkpoint_result(
        &DeleteCheckpointResult {
            name: args.name,
            deleted,
        },
        args.output,
    )?;
    Ok(())
}

fn unknown_checkpoint(
    dir: &std::path::Path,
    name: &str,
) -> Result<paredit_core_cli::CliError, paredit_core_cli::CliError> {
    let available = list_checkpoint_names(dir)?.join(", ");
    Ok(ArgumentError::UnknownName {
        what: "checkpoint",
        name: name.to_owned(),
        available,
    }
    .into())
}
