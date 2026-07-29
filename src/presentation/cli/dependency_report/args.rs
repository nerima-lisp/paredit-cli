use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, GraphFormat, OutputFormat};

#[derive(Debug, Args)]
pub struct DependencyReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Draw the dependency graph instead of reporting it, as Graphviz DOT or a
    /// Mermaid flowchart. Replaces --output.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub graph: Option<GraphFormat>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
