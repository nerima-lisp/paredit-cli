use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::sexpr::SymbolName;

#[derive(Debug, Args)]
pub struct SymbolQueryArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact symbol atom to find.
    #[arg(long)]
    pub symbol: SymbolName,
    /// Exit non-zero unless at least this many occurrences are found.
    #[arg(long)]
    pub require_occurrences: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct SymbolReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact symbol atom to report.
    #[arg(long)]
    pub symbol: SymbolName,
    /// Exit non-zero unless at least this many occurrences are found.
    #[arg(long)]
    pub require_occurrences: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
