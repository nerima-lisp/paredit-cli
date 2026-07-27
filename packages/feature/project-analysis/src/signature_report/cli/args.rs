use std::path::PathBuf;

use clap::Args;

use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SignatureReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::signature_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::signature_report) dialect: Option<DialectArg>,
    /// Exact callable symbol to report. Reports every non-definition call when omitted.
    #[arg(long)]
    pub(in crate::presentation::cli::signature_report) symbol: Option<SymbolName>,
    /// Exit with failure when any discovered call has too few or too many arguments.
    #[arg(long)]
    pub(in crate::presentation::cli::signature_report) fail_on_mismatch: bool,
    /// Require at least this many matching callable definitions.
    #[arg(long)]
    pub(in crate::presentation::cli::signature_report) require_definitions: Option<usize>,
    /// Require at least this many discovered call sites.
    #[arg(long)]
    pub(in crate::presentation::cli::signature_report) require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::signature_report) output: OutputFormat,
}
