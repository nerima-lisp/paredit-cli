use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct NarrowingReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only narrowings of this binding name.
    #[arg(long, value_name = "SYMBOL")]
    pub binding: Option<String>,
    /// Exit with failure when a modelled file narrows nothing anywhere.
    #[arg(long)]
    pub fail_on_none: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
