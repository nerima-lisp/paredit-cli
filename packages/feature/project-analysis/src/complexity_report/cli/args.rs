use clap::Args;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ComplexityReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when any definition's nesting depth exceeds this value.
    #[arg(long)]
    pub fail_on_max_depth: Option<usize>,
    /// Limit the cross-file ranked leaderboard to this many entries.
    #[arg(long)]
    pub top: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
