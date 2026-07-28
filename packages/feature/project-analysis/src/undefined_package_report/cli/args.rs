use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
pub struct UndefinedPackageReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when any in-package form names an undeclared package.
    #[arg(long)]
    pub fail_on_undefined: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
