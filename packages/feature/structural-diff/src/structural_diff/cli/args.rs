use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct StructuralDiffArgs {
    /// The document to compare from.
    pub old: PathBuf,
    /// The document to compare to.
    pub new: PathBuf,
    /// Override extension-based dialect detection for both files.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report only changes at or above this depth. `0` is a whole top-level
    /// form; a higher floor hides the deep edits and leaves the shape changes.
    #[arg(long, value_name = "DEPTH")]
    pub max_depth: Option<usize>,
    /// Exit with failure when the two documents differ structurally. A CI gate
    /// for "this rewrite changed only formatting".
    #[arg(long)]
    pub fail_on_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
