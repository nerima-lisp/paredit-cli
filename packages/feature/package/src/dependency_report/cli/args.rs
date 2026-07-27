use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct DependencyReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::dependency_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::dependency_report) dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::dependency_report) output: OutputFormat,
}
