use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};

#[derive(Debug, Args)]
pub struct AccessorArityReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when any accessor call has the wrong argument count.
    #[arg(long)]
    pub fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
}
