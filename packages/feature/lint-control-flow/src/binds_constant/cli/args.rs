use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct BindsConstantReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::binds_constant_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::binds_constant_report) dialect: Option<DialectArg>,
    /// Exit with failure when any binding names a constant (nil, t, or a keyword).
    #[arg(long)]
    pub(in crate::presentation::cli::binds_constant_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::binds_constant_report) output: OutputFormat,
}
