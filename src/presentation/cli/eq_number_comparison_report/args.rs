use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct EqNumberComparisonReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::eq_number_comparison_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::eq_number_comparison_report) dialect: Option<DialectArg>,
    /// Exit with failure when any eq call compares against a number literal.
    #[arg(long)]
    pub(in crate::presentation::cli::eq_number_comparison_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::eq_number_comparison_report) output: OutputFormat,
}
