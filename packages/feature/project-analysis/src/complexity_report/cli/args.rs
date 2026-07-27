use super::super::*;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct ComplexityReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Exit with failure when any definition's nesting depth exceeds this value.
    #[arg(long)]
    pub(super) fail_on_max_depth: Option<usize>,
    /// Limit the cross-file ranked leaderboard to this many entries.
    #[arg(long)]
    pub(super) top: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}
