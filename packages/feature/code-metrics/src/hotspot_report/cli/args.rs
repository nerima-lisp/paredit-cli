use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};
use paredit_core_cli::runtime::Verbosity;

#[derive(Debug, Args)]
pub struct HotspotReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// How far back to count commits, in `git log --since` syntax.
    #[arg(long, default_value = "1 year ago", value_name = "GIT_DATE")]
    pub since: String,
    /// Exit with failure when a definition scores above this.
    #[arg(long, value_name = "SCORE")]
    pub max_score: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}
