use clap::Args;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct NamingReportArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when any definition name uses a non-idiomatic style.
    #[arg(long)]
    pub fail_on_non_idiomatic: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
