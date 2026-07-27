use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct DuplicateCondTestReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::duplicate_cond_test_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::duplicate_cond_test_report) dialect: Option<DialectArg>,
    /// Exit with failure when any cond form repeats a test expression.
    #[arg(long)]
    pub(in crate::presentation::cli::duplicate_cond_test_report) fail_on_duplicate: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::duplicate_cond_test_report) output: OutputFormat,
}
