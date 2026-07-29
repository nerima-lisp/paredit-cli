use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct StructuralPatchArgs {
    /// The document the change was made from.
    #[arg(long, value_name = "FILE")]
    pub from: PathBuf,
    /// The document the change was made to. `--from` and `--to` together are
    /// the change; neither is written.
    #[arg(long, value_name = "FILE")]
    pub to: PathBuf,
    /// The document to carry the change onto.
    #[arg(long, value_name = "FILE")]
    pub apply_to: PathBuf,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Apply a change at every site that matches, instead of reporting it as
    /// ambiguous and applying it nowhere.
    #[arg(long)]
    pub all: bool,
    /// Write the patched document back to --apply-to. Without it this command
    /// only plans: nothing is written and the plan is the output.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what --write would do, instead of the plan.
    #[arg(long)]
    pub diff: bool,
    /// Exit with failure when any change could not be carried over, so a
    /// scripted port cannot report success on a partial one.
    #[arg(long)]
    pub fail_on_unapplied: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
