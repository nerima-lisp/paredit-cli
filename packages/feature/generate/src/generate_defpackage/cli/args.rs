use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct GenerateDefpackageArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection. Only common-lisp is
    /// supported; every other dialect is refused.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// The package name to generate. Defaults to --file's stem, with
    /// underscores turned into hyphens.
    #[arg(long)]
    pub package_name: Option<String>,
    /// Insert or replace the defpackage form in --file instead of printing a
    /// plan.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what --write would do, instead of the plan.
    #[arg(long)]
    pub diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
