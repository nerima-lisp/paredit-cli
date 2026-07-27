use std::path::PathBuf;

use clap::Args;

use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct CallGraphArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::call_graph_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) dialect: Option<DialectArg>,
    /// Exact callable symbol to focus on as caller or callee.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) symbol: Option<SymbolName>,
    /// Include calls to symbols that have no definition in the scanned file set.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) include_external: bool,
    /// Exit with failure when the focused symbol has inbound internal caller edges.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) fail_on_inbound_callers: bool,
    /// Require at least this many reported call graph edges.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) require_edges: Option<usize>,
    /// Require at least this many reported internal call graph edges.
    #[arg(long)]
    pub(in crate::presentation::cli::call_graph_report) require_internal_edges: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::call_graph_report) output: OutputFormat,
}
