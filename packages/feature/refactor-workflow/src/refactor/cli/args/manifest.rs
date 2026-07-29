use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
#[command(
    after_help = "Reversal:\n  --undo-out writes a journal of reverse edits; `paredit refactor undo --journal <path> --write` puts the files back.\n  --verify-command runs a command after the write and restores every written file when it exits non-zero.\n\nThe verification command runs through the platform shell, in the current directory, with the environment this process was given."
)]
pub struct RefactorApplyArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Rewrite changed files after manifest, hash, and parse gates pass.
    #[arg(long)]
    pub write: bool,
    /// Write a reverse-edit journal here, so the write can be undone later.
    #[arg(long, value_name = "PATH", requires = "write")]
    pub undo_out: Option<PathBuf>,
    /// Run this command after writing; restore every written file if it fails.
    #[arg(long, value_name = "COMMAND", requires = "write")]
    pub verify_command: Option<String>,
    /// Wall-clock budget for --verify-command, in milliseconds.
    #[arg(long, value_name = "MILLIS", requires = "verify_command")]
    pub verify_timeout_ms: Option<u64>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor undo --journal .paredit/undo-rename.json\n  paredit refactor undo --journal .paredit/undo-rename.json --write\n\nWithout --write this reports whether every file can still be put back and changes nothing.\nA journal cannot be applied twice: after a successful undo the files no longer hash to what the write produced."
)]
pub struct RefactorUndoArgs {
    /// Reverse-edit journal written by `refactor apply --undo-out`.
    #[arg(long)]
    pub journal: PathBuf,
    /// Restrict journal file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Restore the recorded pre-image. Without it this only reports.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RefactorCheckArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RefactorDiffArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RefactorStatusArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
