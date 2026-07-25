use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RedefinitionReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::redefinition_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::redefinition_report) dialect: Option<DialectArg>,
    /// Exit with failure when any definition is declared more than once.
    #[arg(long)]
    pub(in crate::presentation::cli::redefinition_report) fail_on_redefinition: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::redefinition_report) output: OutputFormat,
}
