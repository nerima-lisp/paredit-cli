use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat, SelectorArgs};

#[derive(Debug, Args)]
pub struct FormReportArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Include the selected source text in the report.
    #[arg(long)]
    pub include_source: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
