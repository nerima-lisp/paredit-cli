use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_syntax::sexpr::SymbolName;

#[derive(Debug, Args)]
pub struct SignatureReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact callable symbol to report. Reports every non-definition call when omitted.
    #[arg(long)]
    pub symbol: Option<SymbolName>,
    /// Exit with failure when any discovered call has too few or too many arguments.
    #[arg(long)]
    pub fail_on_mismatch: bool,
    /// Require at least this many matching callable definitions.
    #[arg(long)]
    pub require_definitions: Option<usize>,
    /// Require at least this many discovered call sites.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
