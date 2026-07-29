use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct GenerateDefgenericArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection. Only common-lisp is
    /// supported; every other dialect is refused.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Generate only the named generic function, instead of every undeclared
    /// one in the file.
    #[arg(long)]
    pub name: Option<String>,
    /// Insert the generated form(s) into --file instead of printing a plan.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what --write would do, instead of the plan.
    #[arg(long)]
    pub diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
