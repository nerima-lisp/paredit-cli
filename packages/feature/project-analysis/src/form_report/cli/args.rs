use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::sexpr::Path;

#[derive(Debug, Args)]
pub struct FormReportArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Selected expression path, such as 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub path: Option<Path>,
    /// Byte offset inside the selected expression.
    #[arg(long, conflicts_with = "path")]
    pub at: Option<usize>,
    /// Include the selected source text in the report.
    #[arg(long)]
    pub include_source: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
