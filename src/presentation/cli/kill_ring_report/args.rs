use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
pub struct KillRingReportArgs {
    /// Kill ring file. Defaults to $PAREDIT_KILL_RING, then .paredit/kill-ring.json.
    #[arg(long)]
    pub ring: Option<PathBuf>,
    /// Discard a corrupted ring and write a fresh empty one. Only acts on a
    /// ring this run has already found corrupted; never automatic, and never
    /// touches a missing or well-formed ring.
    #[arg(long)]
    pub repair_reset: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}
