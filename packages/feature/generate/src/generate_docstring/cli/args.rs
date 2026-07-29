use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{CompactSelectorArgs, DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct GenerateDocstringArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection. Only common-lisp is
    /// supported; every other dialect is refused.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[command(flatten)]
    pub selector: CompactSelectorArgs,
    /// Insert the generated docstring into --file instead of printing a plan.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what --write would do, instead of the plan.
    #[arg(long)]
    pub diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
