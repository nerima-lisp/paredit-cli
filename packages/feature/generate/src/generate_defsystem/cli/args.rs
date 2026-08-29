use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct GenerateDefsystemArgs {
    /// Directory to scan for Lisp sources.
    pub directory: PathBuf,
    /// Override extension-based dialect detection for every file. Only
    /// common-lisp files contribute; every other file is skipped and
    /// reported.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// The system name to generate. Defaults to --directory's base name,
    /// with underscores turned into hyphens.
    #[arg(long)]
    pub name: Option<String>,
    /// Write the generated form to `<directory>/<name>.asd` instead of only
    /// printing a plan.
    #[arg(long)]
    pub write: bool,
    /// With --write, overwrite `<name>.asd` if it already exists. Without it,
    /// an existing file is left untouched and the command refuses.
    #[arg(long)]
    pub force: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
