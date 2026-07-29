use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct GenerateTestsArgs {
    /// Files or directories to scan for untested definitions.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file. Only
    /// common-lisp is supported; every other dialect is refused.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Append the generated skeletons to this file instead of only printing
    /// them. Required with --write.
    #[arg(long, requires = "write")]
    pub into: Option<PathBuf>,
    /// Append the generated skeletons to --into. Without this flag, only a
    /// plan is printed and nothing is written.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
