use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit inspect workspace .\n  paredit inspect workspace --include-hidden --max-depth 2 ."
)]
pub struct WorkspaceReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// Include files whose extension does not identify a known Lisp dialect.
    #[arg(long)]
    pub include_unknown: bool,
    /// Include hidden directories and files.
    #[arg(long)]
    pub include_hidden: bool,
    /// Include generated or dependency directories such as target and node_modules.
    #[arg(long)]
    pub include_generated: bool,
    /// Maximum directory recursion depth from each root directory.
    #[arg(long)]
    pub max_depth: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
