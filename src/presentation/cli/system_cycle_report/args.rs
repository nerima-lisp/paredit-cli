use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SystemCycleReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::system_cycle_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::system_cycle_report) dialect: Option<DialectArg>,
    /// Exit with failure when any system dependency cycle is found.
    #[arg(long)]
    pub(in crate::presentation::cli::system_cycle_report) fail_on_cycle: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::system_cycle_report) output: OutputFormat,
}
