use std::path::PathBuf;

use clap::Args;
use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub struct TestRiskReportArgs {
    /// Files or directories to scan using static test-name linkage, not execution coverage.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Fail when any modelled function has no naming-convention test link, independent of --top.
    #[arg(long)]
    pub fail_on_untested: bool,
    /// Limit displayed entries only; 0 prints the summary without ranked entries.
    #[arg(long)]
    pub top: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
