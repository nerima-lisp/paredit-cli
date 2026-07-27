use std::path::PathBuf;

use clap::Args;

use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct CallReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::call_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::call_report) dialect: Option<DialectArg>,
    /// Exact list-head symbol to report. Reports every non-definition call when omitted.
    #[arg(long)]
    pub(in crate::presentation::cli::call_report) symbol: Option<SymbolName>,
    /// Include definition-like forms such as defun and defmacro in the report.
    #[arg(long)]
    pub(in crate::presentation::cli::call_report) include_definitions: bool,
    /// Exit non-zero unless at least this many call sites are found.
    #[arg(long)]
    pub(in crate::presentation::cli::call_report) require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::call_report) output: OutputFormat,
}
