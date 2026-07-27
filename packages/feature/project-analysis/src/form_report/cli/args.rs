use std::path::PathBuf;

use clap::Args;

use crate::domain::sexpr::Path;
use crate::presentation::cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct FormReportArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(in crate::presentation::cli::form_report) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(in crate::presentation::cli::form_report) dialect: Option<DialectArg>,
    /// Selected expression path, such as 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub(in crate::presentation::cli::form_report) path: Option<Path>,
    /// Byte offset inside the selected expression.
    #[arg(long, conflicts_with = "path")]
    pub(in crate::presentation::cli::form_report) at: Option<usize>,
    /// Include the selected source text in the report.
    #[arg(long)]
    pub(in crate::presentation::cli::form_report) include_source: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::form_report) output: OutputFormat,
}
