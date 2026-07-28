use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct ConstantReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only folds that remove at least this many bytes.
    #[arg(long, default_value_t = 0)]
    pub min_saved_bytes: i64,
    /// Exit with failure when any expression need not be computed at run time.
    #[arg(long)]
    pub fail_on_foldable: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
