use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::OutputFormat;

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit inspect workspace .\n  paredit inspect workspace --include-hidden --max-depth 2 ."
)]
pub(in crate::presentation::cli) struct WorkspaceReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(super) roots: Vec<PathBuf>,
    /// Include files whose extension does not identify a known Lisp dialect.
    #[arg(long)]
    pub(super) include_unknown: bool,
    /// Include hidden directories and files.
    #[arg(long)]
    pub(super) include_hidden: bool,
    /// Include generated or dependency directories such as target and node_modules.
    #[arg(long)]
    pub(super) include_generated: bool,
    /// Maximum directory recursion depth from each root directory.
    #[arg(long)]
    pub(super) max_depth: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}
