use super::root::RefactorRootReport;
use std::path::PathBuf;

/// What one undo did, or refused to do, per file.
#[derive(Debug)]
pub struct RefactorUndoFileResult {
    pub path: PathBuf,
    /// Whether the journal entry describes an actual change.
    pub changes_content: bool,
    /// Whether the file on disk is byte-for-byte what the write produced.
    ///
    /// The precondition for restoring it. A `false` here means somebody has
    /// edited the file since — including a previous run of this same undo.
    pub current_matches_journal: bool,
    pub restored: bool,
    /// Why this file was left alone, when it was.
    pub blocked_reason: Option<String>,
}

#[derive(Debug)]
pub struct RefactorUndoSummary {
    pub file_count: usize,
    pub changed_file_count: usize,
    pub restorable_file_count: usize,
    pub restored_file_count: usize,
    pub blocked_file_count: usize,
    /// Whether every restorable file was actually restored.
    pub applied: bool,
}

#[derive(Debug)]
pub struct RefactorUndoResult {
    pub journal_path: PathBuf,
    /// The command the journal was recorded for.
    pub operation: String,
    /// The build that recorded it. An undo is only meaningful against the edit
    /// semantics that produced the write.
    pub tool_version: String,
    pub root: RefactorRootReport,
    pub write_requested: bool,
    pub files: Vec<RefactorUndoFileResult>,
    pub summary: RefactorUndoSummary,
}

impl RefactorUndoResult {
    /// Whether every file the journal describes can be put back.
    ///
    /// An undo is all-or-nothing on purpose: restoring half a refactor leaves
    /// a tree in a state nobody designed, which is worse than either endpoint.
    #[must_use]
    pub const fn can_apply(&self) -> bool {
        self.summary.blocked_file_count == 0 && self.summary.changed_file_count > 0
    }

    /// The one-line reason an undo is not possible, for the caller's error.
    #[must_use]
    pub fn blocked_summary(&self) -> String {
        self.files
            .iter()
            .filter_map(|file| {
                file.blocked_reason
                    .as_ref()
                    .map(|reason| format!("{}: {reason}", file.path.display()))
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
