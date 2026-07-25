use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SelfAssignmentReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::self_assignment_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::self_assignment_report) dialect: Option<DialectArg>,
    /// Exit with failure when any self-assignment is found.
    #[arg(long)]
    pub(in crate::presentation::cli::self_assignment_report) fail_on_self_assignment: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::self_assignment_report) output: OutputFormat,
}
