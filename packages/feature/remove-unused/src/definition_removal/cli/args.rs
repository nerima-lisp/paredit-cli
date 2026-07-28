use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::sexpr::Path;

#[derive(Debug, Args)]
pub struct RemoveDefinitionArgs {
    /// File containing the top-level definition.
    #[arg(long)]
    pub file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Top-level definition path from definition-report or unused-definition-report, for example 2.
    #[arg(long)]
    pub path: Path,
    /// Rewrite the file. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RemoveUnusedDefinitionsArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Also remove package/system/test/customization/mode/struct
    /// definitions, and definitions from unrecognized `define-*`-style
    /// macros whose expansion (and any symbol names it derives from the
    /// argument) this tool cannot verify.
    #[arg(long)]
    pub include_protected: bool,
    /// Also remove definitions exported from their Common Lisp package.
    #[arg(long)]
    pub include_exported: bool,
    /// Rewrite files. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
