use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, GraphFormat, ReportFormat};
use paredit_core_cli::runtime::Verbosity;

#[derive(Debug, Args)]
pub struct ClassHierarchyReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when a subclass redeclares a slot a superclass declares.
    #[arg(long)]
    pub fail_on_shadowed_slot: bool,
    /// Draw the class hierarchy instead of reporting it, as Graphviz DOT or a
    /// Mermaid flowchart. Replaces --output; the gate still applies.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub graph: Option<GraphFormat>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}
