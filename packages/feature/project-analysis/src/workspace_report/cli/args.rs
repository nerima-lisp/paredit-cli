use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit inspect workspace .\n  paredit inspect workspace --include-hidden --max-depth 2 .\n  paredit inspect workspace --since origin/main .\n  paredit inspect workspace --exclude-glob 'vendor/**' ."
)]
pub struct WorkspaceReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// Every input selector and filter this tool understands.
    ///
    /// Flattened rather than restated: `--include-unknown`, `--include-hidden`,
    /// `--include-generated` and `--max-depth` used to be declared here, and a
    /// private copy is how one command's default drifts from the others'.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
