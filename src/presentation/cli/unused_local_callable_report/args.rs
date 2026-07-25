use super::super::*;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct UnusedLocalCallableReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Exit with failure when any flet/labels binding is never called.
    #[arg(long)]
    pub(super) fail_on_unused: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}
