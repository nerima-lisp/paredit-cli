use std::path::PathBuf;

use clap::Args;
use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
pub struct DotfilesReportArgs {
    /// Project root whose .paredit metadata is inventoried without reading file bodies.
    #[arg(long)]
    pub root: PathBuf,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
