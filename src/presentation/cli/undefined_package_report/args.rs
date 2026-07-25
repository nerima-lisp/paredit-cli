use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct UndefinedPackageReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::undefined_package_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::undefined_package_report) dialect: Option<DialectArg>,
    /// Exit with failure when any in-package form names an undeclared package.
    #[arg(long)]
    pub(in crate::presentation::cli::undefined_package_report) fail_on_undefined: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::undefined_package_report) output: OutputFormat,
}
