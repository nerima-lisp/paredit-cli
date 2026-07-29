use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::{GraphFormat, OutputFormat};
use paredit_core_syntax::sexpr::SymbolName;

#[derive(Debug, Args)]
pub struct CallGraphArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact callable symbol to focus on as caller or callee.
    #[arg(long)]
    pub symbol: Option<SymbolName>,
    /// Include calls to symbols that have no definition in the scanned file set.
    #[arg(long)]
    pub include_external: bool,
    /// Exit with failure when the focused symbol has inbound internal caller edges.
    #[arg(long)]
    pub fail_on_inbound_callers: bool,
    /// Require at least this many reported call graph edges.
    #[arg(long)]
    pub require_edges: Option<usize>,
    /// Require at least this many reported internal call graph edges.
    #[arg(long)]
    pub require_internal_edges: Option<usize>,
    /// Draw the call graph instead of reporting it, as Graphviz DOT or a
    /// Mermaid flowchart. Replaces --output; the gate still applies.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub graph: Option<GraphFormat>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
