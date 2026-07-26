use clap::Args;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct DuplicateReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Minimum number of matching forms required for a reported group.
    #[arg(long, default_value_t = 2)]
    pub min_group_size: usize,
    /// Minimum expression node count for a candidate form.
    #[arg(long, default_value_t = 4)]
    pub min_node_count: usize,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ReplacementPlanArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Minimum number of matching forms required in one file for a batch.
    #[arg(long, default_value_t = 2)]
    pub min_group_size: usize,
    /// Minimum expression node count for a candidate form.
    #[arg(long, default_value_t = 4)]
    pub min_node_count: usize,
    /// Placeholder replacement form for generated replace-forms commands; review before applying.
    #[arg(long, default_value = "(__review_replacement__)")]
    pub replacement: String,
    /// Keep the first matching form as the canonical sample and replace only later duplicates.
    #[arg(long)]
    pub keep_first: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
