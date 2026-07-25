use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct ExhaustiveCaseOtherwiseReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::exhaustive_case_otherwise_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::exhaustive_case_otherwise_report) dialect: Option<DialectArg>,
    /// Exit with failure when any ecase/ccase/etypecase/ctypecase has a t/otherwise clause.
    #[arg(long)]
    pub(in crate::presentation::cli::exhaustive_case_otherwise_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::exhaustive_case_otherwise_report) output: OutputFormat,
}
