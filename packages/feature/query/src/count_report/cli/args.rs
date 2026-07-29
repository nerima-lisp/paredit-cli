use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  \
      paredit query count --query '(defun ?n ...)' src/\n  \
      paredit query count --query '(if ?t ?a nil)' --query '(when ?t ?a)' .\n  \
      paredit query count --query '(loop ...)' --per-file --output text .")]
pub struct QueryCountArgs {
    /// Files or directories to search recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// A pattern to count. Repeat it to compare several side by side.
    #[arg(long, value_name = "PATTERN", required = true)]
    pub query: Vec<String>,
    /// Every input selector and filter this tool understands.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Break the totals down per file, not just per pattern.
    #[arg(long)]
    pub per_file: bool,
    /// With --per-file, keep the files no pattern reached.
    ///
    /// Off by default: over a repository the zero rows are the overwhelming
    /// majority, and a caller who wants "which files have none of these" is
    /// asking a question `inspect sources` answers better.
    #[arg(long, requires = "per_file")]
    pub include_empty: bool,
    /// Exit with failure when any pattern matches anywhere.
    #[arg(long)]
    pub fail_on_match: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
