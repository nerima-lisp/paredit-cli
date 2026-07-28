use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::sexpr::SymbolName;

#[derive(Debug, Args)]
pub struct CallReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact list-head symbol to report. Reports every non-definition call when omitted.
    #[arg(long)]
    pub symbol: Option<SymbolName>,
    /// Include definition-like forms such as defun and defmacro in the report.
    #[arg(long)]
    pub include_definitions: bool,
    /// Exit non-zero unless at least this many call sites are found.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
