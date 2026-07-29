use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit inspect sources .\n  paredit inspect sources --since origin/main .\n  git ls-files -z | paredit inspect sources --paths-from - --paths-from-separator nul .\n  paredit inspect sources --from-manifest .\n  paredit inspect sources --cache-dir .paredit-cache --list-files ."
)]
pub struct SourceReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// Every input selector and filter this tool understands.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// List every selected file, not just the counts.
    #[arg(long)]
    pub list_files: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
