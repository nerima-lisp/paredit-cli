use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct IdenticalIfBranchReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::identical_if_branch_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::identical_if_branch_report) dialect: Option<DialectArg>,
    /// Exit with failure when any if form has identical branches.
    #[arg(long)]
    pub(in crate::presentation::cli::identical_if_branch_report) fail_on_identical: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::identical_if_branch_report) output: OutputFormat,
}
