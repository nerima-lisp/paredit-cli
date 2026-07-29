use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
pub struct WritabilityReportArgs {
    /// Path to check. Need not exist yet.
    #[arg(long)]
    pub file: PathBuf,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}
