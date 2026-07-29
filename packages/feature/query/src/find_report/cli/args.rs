use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

use crate::find_report::usecase::DEFAULT_PREVIEW_BYTES;

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  \
      paredit query find --query '(defun ?name ...)' src/\n  \
      paredit query find --query '(eq ?x ?x)' --fail-on-match .\n  \
      paredit query find --query '(if ?t ?a nil)' --since origin/main .\n  \
      paredit query find --query \"#'?fn\" --output sarif .")]
pub struct QueryFindArgs {
    /// Files or directories to search recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// The S-expression pattern to search for. See `paredit inspect resolve`
    /// and the selector guide for the language.
    #[arg(long, value_name = "PATTERN")]
    pub query: String,
    /// Every input selector and filter this tool understands.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Bytes of source text to show per match.
    #[arg(long, value_name = "N", default_value_t = DEFAULT_PREVIEW_BYTES)]
    pub preview_bytes: usize,
    /// Exit with failure when the pattern matches anywhere (a banned shape).
    #[arg(long)]
    pub fail_on_match: bool,
    /// Exit with failure when the pattern matches nowhere (a required shape).
    #[arg(long, conflicts_with = "fail_on_match")]
    pub fail_on_no_match: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
}
