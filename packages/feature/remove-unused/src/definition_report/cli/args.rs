use super::super::*;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct DefinitionReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct UnusedDefinitionReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Exit with failure when at least one externally unreferenced definition is found.
    #[arg(long)]
    pub(super) fail_on_unused: bool,
    /// Require at least this many externally unreferenced definitions.
    #[arg(long)]
    pub(super) require_unused_definitions: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}
