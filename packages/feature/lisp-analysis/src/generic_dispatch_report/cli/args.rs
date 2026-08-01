use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};
use paredit_core_cli::runtime::Verbosity;

#[derive(Debug, Args)]
pub struct GenericDispatchReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when a generic function and its methods disagree.
    #[arg(long)]
    pub fail_on_defect: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}
