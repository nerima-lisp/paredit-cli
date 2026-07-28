use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct ValuePropagationReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only the bindings propagation could not resolve, with the reason.
    #[arg(long)]
    pub blocked_only: bool,
    /// Exit with failure when the resolved/seen ratio falls below this value.
    #[arg(long, value_name = "RATIO")]
    pub min_coverage: Option<f64>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
