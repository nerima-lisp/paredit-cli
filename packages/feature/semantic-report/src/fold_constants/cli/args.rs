use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct FoldConstantsArgs {
    /// Files or directories to fold.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Fold only expressions that remove at least this many bytes.
    ///
    /// The same filter `inspect constants` takes, and the reason it is here:
    /// folding `(+ 1 2)` to `3` is a clear win, and folding a short form to a
    /// longer string literal is not, so a caller who wants only the profitable
    /// ones says so with a positive value.
    #[arg(long, default_value_t = 0)]
    pub min_saved_bytes: i64,
    /// Write the folded documents back instead of only reporting the plan.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff per changed file.
    #[arg(long)]
    pub diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
