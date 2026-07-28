use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
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
