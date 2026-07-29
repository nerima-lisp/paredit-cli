use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  \
      paredit query replace --query '(if ?t ?a nil)' --rewrite '(when ?t ?a)' src/\n  \
      paredit query replace --query '(if ?t ?a nil)' --rewrite '(when ?t ?a)' --diff src/\n  \
      paredit query replace --query '(old-name ?args...)' --rewrite '(new-name ?args...)' --write .\n  \
      paredit query replace --query '(deprecated ...)' --rewrite '(current)' --check .\n\n\
      Nothing is written without --write. A rewrite reflows the matched form onto \n\
      the template's own layout, so `paredit edit format --write` afterwards is usual.")]
pub struct QueryReplaceArgs {
    /// Files or directories to search recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// The S-expression pattern naming the forms to replace.
    #[arg(long, value_name = "PATTERN")]
    pub query: String,
    /// The template each match becomes. `?name` writes back what the pattern's
    /// `?name` captured, verbatim.
    #[arg(long, value_name = "TEMPLATE")]
    pub rewrite: String,
    /// Every input selector and filter this tool understands.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Apply the replacements in place.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what would change.
    #[arg(long)]
    pub diff: bool,
    /// Write nothing and exit 3 if any replacement is pending (a CI gate).
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
    /// Rewrite matches even when doing so deletes a comment that no capture
    /// carries over.
    ///
    /// Off by default. A comment lives outside the node tree, so a rewrite
    /// that drops one still parses and still looks right; the loss is only
    /// visible to whoever wrote the comment.
    #[arg(long)]
    pub allow_comment_loss: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
