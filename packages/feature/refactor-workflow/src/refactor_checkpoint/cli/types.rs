use std::path::PathBuf;

/// What `create-checkpoint` did.
#[derive(Debug)]
pub struct CreateCheckpointResult {
    pub name: String,
    pub created_at_unix: u64,
    pub replaced_existing: bool,
    pub files: Vec<PathBuf>,
}

/// One checkpoint, as `list-checkpoints` reports it.
#[derive(Debug)]
pub struct CheckpointSummary {
    pub name: String,
    pub created_at_unix: u64,
    pub files: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct ListCheckpointsResult {
    pub checkpoints_dir: PathBuf,
    pub checkpoints: Vec<CheckpointSummary>,
}

/// What one file in a checkpoint would do, or did, on restore.
#[derive(Debug)]
pub struct CheckpointRestoreFileResult {
    pub path: PathBuf,
    /// Whether the file on disk is byte-for-byte what the checkpoint
    /// recorded. `false` means either this tool or a person changed it since
    /// the checkpoint was taken — the two are indistinguishable by design,
    /// which is what makes refusing safe by default.
    pub matches_checkpoint: bool,
    pub restored: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug)]
pub struct CheckpointRestoreSummary {
    pub file_count: usize,
    pub restorable_file_count: usize,
    pub blocked_file_count: usize,
    pub restored_file_count: usize,
    pub applied: bool,
}

#[derive(Debug)]
pub struct CheckpointRestoreResult {
    pub name: String,
    pub created_at_unix: u64,
    pub write_requested: bool,
    pub files: Vec<CheckpointRestoreFileResult>,
    pub summary: CheckpointRestoreSummary,
}

impl CheckpointRestoreResult {
    /// Whether every file the checkpoint covers can be put back.
    #[must_use]
    pub const fn can_restore(&self) -> bool {
        self.summary.blocked_file_count == 0 && self.summary.file_count > 0
    }

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

/// What `delete-checkpoint` did.
#[derive(Debug)]
pub struct DeleteCheckpointResult {
    pub name: String,
    pub deleted: bool,
}
