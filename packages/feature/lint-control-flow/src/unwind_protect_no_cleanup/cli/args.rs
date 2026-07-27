use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct UnwindProtectNoCleanupReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::unwind_protect_no_cleanup_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::unwind_protect_no_cleanup_report) dialect: Option<DialectArg>,
    /// Exit with failure when any cleanupless `(unwind-protect x)` is found.
    #[arg(long)]
    pub(in crate::presentation::cli::unwind_protect_no_cleanup_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::unwind_protect_no_cleanup_report) output: OutputFormat,
}
