use super::super::*;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct DuplicateReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Minimum number of matching forms required for a reported group.
    #[arg(long, default_value_t = 2)]
    pub(super) min_group_size: usize,
    /// Minimum expression node count for a candidate form.
    #[arg(long, default_value_t = 4)]
    pub(super) min_node_count: usize,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct ReplacementPlanArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Minimum number of matching forms required in one file for a batch.
    #[arg(long, default_value_t = 2)]
    pub(super) min_group_size: usize,
    /// Minimum expression node count for a candidate form.
    #[arg(long, default_value_t = 4)]
    pub(super) min_node_count: usize,
    /// Placeholder replacement form for generated replace-forms commands; review before applying.
    #[arg(long, default_value = "(__review_replacement__)")]
    pub(super) replacement: String,
    /// Keep the first matching form as the canonical sample and replace only later duplicates.
    #[arg(long)]
    pub(super) keep_first: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}
