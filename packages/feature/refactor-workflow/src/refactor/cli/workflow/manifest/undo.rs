//! Putting a written refactor back.
//!
//! `refactor apply --write` is the one command in this tool that changes files
//! it was not pointed at individually, and until now the only way back was the
//! version control system. That is usually enough and it is not always: a
//! refactor applied on top of uncommitted work cannot be reverted with `git
//! checkout` without taking the uncommitted work with it.
//!
//! A journal makes the reversal a first-class operation with the same guards
//! as the write it undoes: the file must be byte-for-byte what the write
//! produced, and the result must be byte-for-byte what the write replaced.
//! Both are checked before anything is written, and the write itself goes
//! through the same transactional writer, so an undo of five files is as
//! all-or-nothing as the apply was.
//!
//! The same function serves `--verify-command`: "the tests failed, put it
//! back" and "the operator asked to put it back" are the same operation, and
//! implementing them separately would mean two rollback paths of which only
//! one is exercised by tests.

use super::super::super::args::RefactorUndoArgs;
use super::super::super::manifest::root::read_refactor_manifest_source;
use super::super::super::render::print_refactor_undo_result;
use super::super::super::types::root::{RefactorRootGuard, RefactorRootReport};
use super::super::super::types::undo::{
    RefactorUndoFileResult, RefactorUndoResult, RefactorUndoSummary,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::{
    ExpectedWriteTarget, write_files_with_rollback_expected,
    write_files_with_rollback_expected_anchored,
};
use paredit_core_safety::journal::UndoJournal;
use std::path::{Path as FsPath, PathBuf};

/// Reads a journal, reports what it can restore, and with `--write` restores it.
pub fn refactor_undo(args: RefactorUndoArgs) -> CliResult<()> {
    let text = paredit_core_cli::shared::read_text_file_with_limit(
        &args.journal,
        paredit_core_cli::shared::MAX_SOURCE_INPUT_BYTES,
    )
    .map_err(crate::error::RefactorContext::new(format!(
        "failed to read undo journal {}",
        args.journal.display()
    )))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| paredit_core_cli::CliError::Json {
            context: format!("undo journal {} is not JSON", args.journal.display()),
            source,
        })?;
    let journal = UndoJournal::from_json(&value).map_err(crate::error::RefactorContext::new(
        format!("undo journal {} is unusable", args.journal.display()),
    ))?;

    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    let plan = plan_undo(&journal, root_guard.as_ref())?;
    let result = RefactorUndoResult {
        journal_path: args.journal.clone(),
        operation: journal.operation.clone(),
        tool_version: journal.tool_version.clone(),
        root: RefactorRootReport::from_guard(root_guard.as_ref()),
        write_requested: args.write,
        files: plan.file_results(args.write),
        summary: plan.summary(args.write),
    };

    if args.write && result.can_apply() {
        commit_restores(plan.restores, root_guard.as_ref())?;
    }

    print_refactor_undo_result(&result, args.output)?;

    if args.write && !result.can_apply() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalTargetChanged,
            format!(
                "refactor undo refused: {}",
                if result.summary.changed_file_count == 0 {
                    "the journal records no change to reverse".to_owned()
                } else {
                    result.blocked_summary()
                }
            ),
        )
        .into());
    }

    Ok(())
}

/// Restores every file a journal describes, for a caller that already has one
/// in hand.
///
/// This is what `--verify-command` calls when the command fails. It reports
/// nothing and either restores everything or fails, because the caller is in
/// the middle of reporting a different failure and a partial rollback would
/// make that report wrong.
pub fn restore_from_journal(
    journal: &UndoJournal,
    root_guard: Option<&RefactorRootGuard>,
) -> CliResult<usize> {
    let plan = plan_undo(journal, root_guard)?;
    if !(plan.blocked.is_empty()) {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalTargetChanged,
            format!(
                "cannot restore: {}",
                plan.blocked
                    .iter()
                    .map(|(path, reason)| format!("{}: {reason}", path.display()))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
        .into());
    }
    let restored = plan.restores.len();
    commit_restores(plan.restores, root_guard)?;
    Ok(restored)
}

/// One file that can be put back, with the guard the writer needs.
type Restore = (PathBuf, String, ExpectedWriteTarget);

/// What an undo would do, decided before anything is written.
struct UndoPlan {
    /// Files that can be restored, in journal order.
    restores: Vec<Restore>,
    /// Files that cannot, with the reason.
    blocked: Vec<(PathBuf, String)>,
    /// Journal entries that record no change; nothing to do and not a problem.
    unchanged: Vec<PathBuf>,
}

impl UndoPlan {
    fn file_results(&self, write_requested: bool) -> Vec<RefactorUndoFileResult> {
        let restorable = self.blocked.is_empty();
        let mut files =
            Vec::with_capacity(self.restores.len() + self.blocked.len() + self.unchanged.len());
        for (path, _, _) in &self.restores {
            files.push(RefactorUndoFileResult {
                path: path.clone(),
                changes_content: true,
                current_matches_journal: true,
                restored: write_requested && restorable,
                blocked_reason: None,
            });
        }
        for (path, reason) in &self.blocked {
            files.push(RefactorUndoFileResult {
                path: path.clone(),
                changes_content: true,
                current_matches_journal: false,
                restored: false,
                blocked_reason: Some(reason.clone()),
            });
        }
        for path in &self.unchanged {
            files.push(RefactorUndoFileResult {
                path: path.clone(),
                changes_content: false,
                current_matches_journal: true,
                restored: false,
                blocked_reason: None,
            });
        }
        // Journal order is not path order, and a report that reorders between
        // runs is not comparable. Sorting here is what makes the output stable.
        files.sort_by(|left, right| left.path.cmp(&right.path));
        files
    }

    fn summary(&self, write_requested: bool) -> RefactorUndoSummary {
        let changed_file_count = self.restores.len() + self.blocked.len();
        RefactorUndoSummary {
            file_count: changed_file_count + self.unchanged.len(),
            changed_file_count,
            restorable_file_count: self.restores.len(),
            restored_file_count: if write_requested && self.blocked.is_empty() {
                self.restores.len()
            } else {
                0
            },
            blocked_file_count: self.blocked.len(),
            applied: write_requested && self.blocked.is_empty() && changed_file_count > 0,
        }
    }
}

/// Works out what each journal entry would do, without writing anything.
///
/// Every entry is examined even after one is found to be blocked, so a report
/// names all the files that drifted rather than only the first.
fn plan_undo(journal: &UndoJournal, root_guard: Option<&RefactorRootGuard>) -> CliResult<UndoPlan> {
    let mut plan = UndoPlan {
        restores: Vec::new(),
        blocked: Vec::new(),
        unchanged: Vec::new(),
    };

    for entry in &journal.files {
        if !entry.changes_content() {
            plan.unchanged.push(entry.path.clone());
            continue;
        }

        let read = read_refactor_manifest_source(&entry.path, root_guard);
        let (resolved_path, current, expected) = match read {
            Ok(read) => read,
            Err(error) => {
                plan.blocked
                    .push((entry.path.clone(), format!("{error:#}")));
                continue;
            }
        };

        match entry.restore(&current) {
            Ok(restored) => plan.restores.push((resolved_path, restored, expected)),
            Err(error) => plan.blocked.push((entry.path.clone(), error.to_string())),
        }
    }

    Ok(plan)
}

/// Writes every restore in one transaction.
fn commit_restores(
    restores: Vec<Restore>,
    root_guard: Option<&RefactorRootGuard>,
) -> CliResult<()> {
    if restores.is_empty() {
        return Ok(());
    }
    match root_guard {
        Some(root_guard) => {
            let anchored = restores
                .into_iter()
                .map(|(path, content, expected)| {
                    root_guard.anchored_manifest_write(path, content, expected)
                })
                .collect::<CliResult<Vec<_>>>()?;
            write_files_with_rollback_expected_anchored(anchored)?;
        }
        None => write_files_with_rollback_expected(restores)?,
    }
    Ok(())
}

/// Where a journal is written when `--undo-out` is a bare directory.
///
/// Exposed so the apply path and any future writer agree on the name.
#[must_use]
pub fn journal_file_name(operation: &str) -> String {
    format!("paredit-undo-{}.json", operation.replace(' ', "-"))
}

/// Resolves `--undo-out` to a file path, allowing it to name a directory.
#[must_use]
pub fn resolve_journal_path(requested: &FsPath, operation: &str) -> PathBuf {
    if requested.is_dir() {
        requested.join(journal_file_name(operation))
    } else {
        requested.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::{journal_file_name, resolve_journal_path};
    use std::path::Path;

    #[test]
    fn a_journal_name_is_derived_from_the_operation() {
        assert_eq!(
            journal_file_name("refactor apply"),
            "paredit-undo-refactor-apply.json"
        );
    }

    #[test]
    fn a_file_path_is_used_as_written() {
        assert_eq!(
            resolve_journal_path(Path::new("/tmp/undo.json"), "refactor apply"),
            Path::new("/tmp/undo.json")
        );
    }
}
