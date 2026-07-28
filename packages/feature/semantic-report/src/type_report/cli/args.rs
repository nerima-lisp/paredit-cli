use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct TypeReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only the bindings and expressions whose declared and inferred
    /// types contradict each other.
    #[arg(long)]
    pub contradictions_only: bool,
    /// Exit with failure when a declaration names a type no object can satisfy.
    #[arg(long)]
    pub fail_on_contradiction: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
