use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct AddIgnoreDeclarationArgs {
    /// Files to rewrite. A directory is expanded to the Lisp sources under it.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Write the rewritten documents back instead of only reporting the plan.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff per changed file.
    #[arg(long)]
    pub diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
