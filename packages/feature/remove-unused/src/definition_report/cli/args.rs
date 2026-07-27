use clap::Args;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct DefinitionReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct UnusedDefinitionReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when at least one externally unreferenced definition is found.
    #[arg(long)]
    pub fail_on_unused: bool,
    /// Require at least this many externally unreferenced definitions.
    #[arg(long)]
    pub require_unused_definitions: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
