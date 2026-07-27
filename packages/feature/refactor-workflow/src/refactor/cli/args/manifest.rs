use std::path::PathBuf;

use clap::Args;

use super::super::super::OutputFormat;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RefactorApplyArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub(in crate::presentation::cli) expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub(in crate::presentation::cli) root: Option<PathBuf>,
    /// Rewrite changed files after manifest, hash, and parse gates pass.
    #[arg(long)]
    pub(in crate::presentation::cli) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RefactorCheckArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub(in crate::presentation::cli) expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub(in crate::presentation::cli) root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RefactorDiffArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub(in crate::presentation::cli) expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub(in crate::presentation::cli) root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RefactorStatusArgs {
    /// JSON manifest emitted by refactor preview or workspace refactor preview.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest: PathBuf,
    /// Refuse to read a manifest whose stable hash differs from this value.
    #[arg(long)]
    pub(in crate::presentation::cli) expect_manifest_hash: Option<String>,
    /// Restrict manifest file paths to this workspace root.
    #[arg(long)]
    pub(in crate::presentation::cli) root: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}
