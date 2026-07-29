use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_cli::args::{DialectArg, OutputFormat};

/// The verdicts a run may restrict its output to.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PurityFilter {
    Pure,
    Effectful,
    Unknown,
}

#[derive(Debug, Args)]
pub struct EffectReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only definitions with this verdict.
    #[arg(long, value_enum)]
    pub purity: Option<PurityFilter>,
    /// Exit with failure when any definition's effects cannot be decided.
    #[arg(long)]
    pub fail_on_unknown: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

impl PurityFilter {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Effectful => "effectful",
            Self::Unknown => "unknown",
        }
    }
}
