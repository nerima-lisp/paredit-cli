use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

/// The checkpoint directory, shared by every command that touches the
/// registry — the same shape [`paredit_core_cli::args::KillRingArgs`] gives
/// the kill ring.
#[derive(Debug, Args)]
pub struct CheckpointStoreArgs {
    /// Checkpoint directory. Defaults to $PAREDIT_CHECKPOINTS_DIR, then
    /// .paredit/checkpoints.
    #[arg(long, value_name = "DIR")]
    pub checkpoints_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor create-checkpoint --name before-rename src/foo.lisp src/bar.lisp\n\nRejects a name that already has a checkpoint unless --force is given."
)]
pub struct CreateCheckpointArgs {
    /// Name for the checkpoint. ASCII letters, digits, `-`, `_` or `.`; must
    /// not start with `.`.
    #[arg(long)]
    pub name: String,
    /// Files to snapshot into the checkpoint.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Restrict file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Replace an existing checkpoint with the same name.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub store: CheckpointStoreArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  paredit refactor list-checkpoints")]
pub struct ListCheckpointsArgs {
    #[command(flatten)]
    pub store: CheckpointStoreArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor restore-checkpoint --name before-rename\n  paredit refactor restore-checkpoint --name before-rename --write\n\nWithout --write this only reports whether every covered file can still be restored.\nRefuses whenever a covered file is not byte-for-byte what the checkpoint recorded, whether the file changed through this tool or by hand."
)]
pub struct RestoreCheckpointArgs {
    /// Name of the checkpoint to restore.
    #[arg(long)]
    pub name: String,
    /// Restrict file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Restore the recorded content. Without it this only reports.
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub store: CheckpointStoreArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  paredit refactor delete-checkpoint --name before-rename")]
pub struct DeleteCheckpointArgs {
    /// Name of the checkpoint to delete.
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub store: CheckpointStoreArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
