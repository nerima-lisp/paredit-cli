use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct DuplicateSetfPlaceReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::duplicate_setf_place_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::duplicate_setf_place_report) dialect: Option<DialectArg>,
    /// Exit with failure when any duplicate setf place is found.
    #[arg(long)]
    pub(in crate::presentation::cli::duplicate_setf_place_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::duplicate_setf_place_report) output: OutputFormat,
}
